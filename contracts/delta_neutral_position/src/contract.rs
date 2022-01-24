use std::cmp::{min, Ordering};

use aperture_common::common::Recipient;
use aperture_common::delta_neutral_position_manager::{self, Context};
use aperture_common::terra_manager;
use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::Item;
use terraswap::asset::{Asset, AssetInfo};

use crate::dex_util::{
    compute_terraswap_offer_amount, get_terraswap_mirror_asset_uusd_liquidity_info,
    simulate_terraswap_swap, swap_cw20_token_for_uusd,
};
use crate::math::{decimal_division, decimal_multiplication, reverse_decimal};
use crate::state::{
    CDP_IDX, MANAGER, MIRROR_ASSET_CW20_ADDR, POSITION_CLOSE_INFO, POSITION_OPEN_INFO,
    TARGET_COLLATERAL_RATIO_RANGE,
};
use crate::util::{
    find_collateral_uusd_amount, find_unclaimed_mir_amount, find_unclaimed_spec_amount,
    get_cdp_uusd_lock_info_result, get_position_state, get_uusd_asset_from_amount,
    get_uusd_balance, increase_mirror_asset_balance_from_long_farm,
    increase_uusd_balance_from_aust_collateral, query_position_info,
    unstake_lp_and_withdraw_liquidity,
};
use aperture_common::delta_neutral_position::{
    ControllerExecuteMsg, ExecuteMsg, InstantiateMsg, InternalExecuteMsg, MigrateMsg,
    PositionActionInfo, QueryMsg, TargetCollateralRatioRange,
};
use aperture_common::delta_neutral_position_manager::QueryMsg as ManagerQueryMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    MANAGER.save(deps.storage, &info.sender)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let manager_addr = MANAGER.load(deps.storage)?;

    // `CONTEXT.query()` uses WasmQuery::RawQuery to load context directly from the storage of `manager_addr`.
    // This helps save gas compared to using smart query as that involves extra JSON serialization / deserialization processes.
    const CONTEXT: Item<Context> = Item::new("context");
    let context = CONTEXT.query(&deps.querier, manager_addr.clone())?;

    // ACL check.
    let is_authorized = match msg {
        ExecuteMsg::Controller(_) => {
            info.sender == context.controller || info.sender == env.contract.address
        }
        ExecuteMsg::Internal(_) => info.sender == env.contract.address,
        _ => info.sender == manager_addr,
    };
    if !is_authorized {
        return Err(StdError::generic_err("unauthorized"));
    }

    match msg {
        ExecuteMsg::OpenPosition { params } => open_position(
            deps,
            env,
            context,
            params.target_min_collateral_ratio,
            params.target_max_collateral_ratio,
            params.mirror_asset_cw20_addr,
        ),
        ExecuteMsg::DecreasePosition {
            proportion,
            recipient,
        } => decrease_position(env, proportion, recipient),
        ExecuteMsg::Controller(controller_msg) => match controller_msg {
            ControllerExecuteMsg::RebalanceAndReinvest {} => {
                rebalance_and_reinvest(deps.as_ref(), env, context)
            }
        },
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
            InternalExecuteMsg::ClaimAndIncreaseUusdBalance {} => {
                claim_and_increase_uusd_balance(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::AchieveDeltaNeutral {} => {
                achieve_delta_neutral(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::AchieveSafeCollateralRatio {} => {
                achieve_safe_collateral_ratios(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::WithdrawFundsInUusd {
                proportion,
                recipient,
            } => withdraw_funds_in_uusd(deps.as_ref(), env, context, proportion, recipient),
            InternalExecuteMsg::WithdrawCollateralAndRedeemForUusd { proportion } => {
                withdraw_collateral_and_redeem_for_uusd(deps.as_ref(), context, proportion)
            }
            InternalExecuteMsg::SendUusdToRecipient {
                proportion,
                recipient,
            } => send_uusd_to_recipient(deps, env, proportion, recipient),
            InternalExecuteMsg::PairUusdWithMirrorAssetToProvideLiquidityAndStake {} => {
                pair_uusd_with_mirror_asset_to_provide_liquidity_and_stake(
                    deps.as_ref(),
                    env,
                    context,
                )
            }
            InternalExecuteMsg::StakeTerraswapLpTokens { lp_token_cw20_addr } => {
                stake_terraswap_lp_tokens(deps.as_ref(), env, context, lp_token_cw20_addr)
            }
            InternalExecuteMsg::DeltaNeutralReinvest {} => {
                let cdp_idx = CDP_IDX.load(deps.storage)?;
                let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
                delta_neutral_invest(deps, env, context, &mirror_asset_cw20_addr, Some(cdp_idx))
            }
        },
    }
}

pub fn create_internal_execute_message(env: &Env, msg: InternalExecuteMsg) -> CosmosMsg {
    CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::Internal(msg)).unwrap(),
        funds: vec![],
    })
}

pub fn get_reinvest_internal_messages(deps: Deps, env: &Env, context: &Context) -> Vec<CosmosMsg> {
    if let Ok(lock_info_response) = get_cdp_uusd_lock_info_result(deps, context) {
        if !lock_info_response.locked_amount.is_zero() {
            return vec![];
        }
    }
    vec![
        create_internal_execute_message(
            env,
            InternalExecuteMsg::PairUusdWithMirrorAssetToProvideLiquidityAndStake {},
        ),
        create_internal_execute_message(env, InternalExecuteMsg::DeltaNeutralReinvest {}),
    ]
}

pub fn rebalance_and_reinvest(deps: Deps, env: Env, context: Context) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(vec![
            create_internal_execute_message(
                &env,
                InternalExecuteMsg::ClaimAndIncreaseUusdBalance {},
            ),
            create_internal_execute_message(&env, InternalExecuteMsg::AchieveDeltaNeutral {}),
            create_internal_execute_message(
                &env,
                InternalExecuteMsg::AchieveSafeCollateralRatio {},
            ),
        ])
        .add_messages(get_reinvest_internal_messages(deps, &env, &context)))
}

pub fn claim_and_increase_uusd_balance(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let spec_reward = find_unclaimed_spec_amount(deps, &env, &context)?;
    let mir_reward = find_unclaimed_mir_amount(deps, &env, &context)?;

    // Claim MIR / SPEC reward and swap them for uusd.
    let mut response = Response::new();
    if spec_reward > Uint128::zero() {
        // Mint SPEC tokens to ensure that emissable SPEC tokens are available for withdrawal.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_gov_addr.to_string(),
            msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {})?,
            funds: vec![],
        }));

        // Claim SPEC reward.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_mirror_farms_addr.to_string(),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::withdraw {
                asset_token: None,
                farm_amount: None,
                spec_amount: None,
            })?,
            funds: vec![],
        }));

        // Swap SPEC reward for uusd.
        response = response.add_message(swap_cw20_token_for_uusd(
            &deps.querier,
            &context.terraswap_factory_addr,
            &context.astroport_factory_addr,
            &context.spectrum_cw20_addr,
            spec_reward,
        )?);
    }
    if mir_reward > Uint128::zero() {
        // Claim MIR reward.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::Withdraw { asset_token: None })?,
            funds: vec![],
        }));

        // Swap MIR for uusd.
        response = response.add_message(swap_cw20_token_for_uusd(
            &deps.querier,
            &context.terraswap_factory_addr,
            &context.astroport_factory_addr,
            &context.mirror_cw20_addr,
            mir_reward,
        )?);
    }

    // If there are any unlocked funds in the short farm, claim them.
    let position_lock_info_result = get_cdp_uusd_lock_info_result(deps, &context);
    if let Ok(position_lock_info_response) = position_lock_info_result {
        if position_lock_info_response.unlock_time <= env.block.time.seconds() {
            response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.mirror_lock_addr.to_string(),
                msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
                    positions_idx: vec![CDP_IDX.load(deps.storage)?],
                })?,
                funds: vec![],
            }));
        }
    }

    Ok(response)
}

pub fn achieve_delta_neutral(deps: Deps, env: Env, context: Context) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;

    if state.mirror_asset_long_farm.is_zero() {
        // There are no staked LP tokens.
        match state
            .mirror_asset_long_amount
            .cmp(&state.mirror_asset_short_amount)
        {
            Ordering::Greater => {
                return Ok(Response::new().add_message(swap_cw20_token_for_uusd(
                    &deps.querier,
                    &context.terraswap_factory_addr,
                    &context.astroport_factory_addr,
                    &mirror_asset_cw20_addr,
                    state.mirror_asset_balance - state.mirror_asset_short_amount,
                )?))
            }
            Ordering::Less => {
                let (pair_info, pool_mirror_asset_amount, pool_uusd_amount) =
                    get_terraswap_mirror_asset_uusd_liquidity_info(
                        deps,
                        &context.terraswap_factory_addr,
                        &mirror_asset_cw20_addr,
                    )?;
                let offer_uusd_amount = compute_terraswap_offer_amount(
                    pool_mirror_asset_amount,
                    pool_uusd_amount,
                    state.mirror_asset_short_amount - state.mirror_asset_balance,
                )?;
                return Ok(
                    Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: pair_info.contract_addr,
                        msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                            offer_asset: get_uusd_asset_from_amount(offer_uusd_amount),
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })?,
                        funds: vec![Coin {
                            denom: String::from("uusd"),
                            amount: offer_uusd_amount,
                        }],
                    })),
                );
            }
            Ordering::Equal => {
                return Ok(Response::default());
            }
        };
    }

    let mut response = Response::new();
    match state
        .mirror_asset_long_amount
        .cmp(&state.mirror_asset_short_amount)
    {
        Ordering::Greater => {
            let info = state.terraswap_pool_info.as_ref().unwrap();
            let need_to_withdraw_lp_token = if state.mirror_asset_balance > Uint128::zero() {
                let new_mirror_asset_pool_amount =
                    info.terraswap_pool_mirror_asset_amount + state.mirror_asset_balance;
                let mirror_asset_long_amount = new_mirror_asset_pool_amount
                    * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply);
                mirror_asset_long_amount > state.mirror_asset_short_amount
            } else {
                false
            };

            let one = Uint128::from(1u128);
            let mut a = Uint128::zero();
            if need_to_withdraw_lp_token {
                let mut b = info.lp_token_amount + one;
                while b > a + one {
                    let withdraw_lp_token_amount = (a + b) >> 1;
                    let fraction =
                        Decimal::from_ratio(withdraw_lp_token_amount, info.lp_token_total_supply);
                    let withdrawn_mirror_asset_amount =
                        info.terraswap_pool_mirror_asset_amount * fraction;
                    let new_lp_total_supply = info.lp_token_total_supply - withdraw_lp_token_amount;
                    let pool_mirror_asset_amount_after_withdrawal =
                        info.terraswap_pool_mirror_asset_amount - withdrawn_mirror_asset_amount;
                    let pool_mirror_asset_amount_after_swap =
                        pool_mirror_asset_amount_after_withdrawal
                            + withdrawn_mirror_asset_amount
                            + state.mirror_asset_balance;
                    let new_long_farm_mirror_asset_amount = pool_mirror_asset_amount_after_swap
                        * Decimal::from_ratio(
                            info.lp_token_amount - withdraw_lp_token_amount,
                            new_lp_total_supply,
                        );
                    if new_long_farm_mirror_asset_amount >= state.mirror_asset_short_amount {
                        a = withdraw_lp_token_amount;
                    } else {
                        b = withdraw_lp_token_amount;
                    }
                }
                response = response
                    .add_messages(unstake_lp_and_withdraw_liquidity(
                        &state,
                        &context,
                        &mirror_asset_cw20_addr,
                        a,
                    ))
                    .add_message(swap_cw20_token_for_uusd(
                        &deps.querier,
                        &context.terraswap_factory_addr,
                        &context.astroport_factory_addr,
                        &mirror_asset_cw20_addr,
                        info.terraswap_pool_mirror_asset_amount
                            * Decimal::from_ratio(a, info.lp_token_total_supply)
                            + state.mirror_asset_balance,
                    )?);
            } else {
                let mut b = state.mirror_asset_balance + one;
                while b > a + one {
                    let offer_mirror_asset_amount = (a + b) >> 1;
                    let pool_mirror_asset_amount_after_swap =
                        info.terraswap_pool_mirror_asset_amount + offer_mirror_asset_amount;
                    let new_mirror_asset_long_amount = state.mirror_asset_balance
                        - offer_mirror_asset_amount
                        + pool_mirror_asset_amount_after_swap
                            * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply);
                    if new_mirror_asset_long_amount >= state.mirror_asset_short_amount {
                        a = offer_mirror_asset_amount;
                    } else {
                        b = offer_mirror_asset_amount;
                    }
                }
                response = response.add_message(swap_cw20_token_for_uusd(
                    &deps.querier,
                    &context.terraswap_factory_addr,
                    &context.astroport_factory_addr,
                    &mirror_asset_cw20_addr,
                    a,
                )?);
            }
        }
        Ordering::Less => {
            let info = state.terraswap_pool_info.as_ref().unwrap();
            let need_to_withdraw_lp_token = if state.uusd_balance > Uint128::zero() {
                let (_, pool_mirror_asset_amount_after_swap, return_mirror_asset_amount) =
                    simulate_terraswap_swap(
                        info.terraswap_pool_uusd_amount,
                        info.terraswap_pool_mirror_asset_amount,
                        state.uusd_balance,
                    );
                let mirror_asset_long_amount = pool_mirror_asset_amount_after_swap
                    * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply)
                    + return_mirror_asset_amount
                    + state.mirror_asset_balance;
                mirror_asset_long_amount < state.mirror_asset_short_amount
            } else {
                false
            };

            let mut a = Uint128::zero();
            let one = Uint128::from(1u128);
            if need_to_withdraw_lp_token {
                let mut b = info.lp_token_amount + one;
                while b > a + one {
                    let withdraw_lp_token_amount = (a + b) >> 1;
                    let fraction =
                        Decimal::from_ratio(withdraw_lp_token_amount, info.lp_token_total_supply);
                    let withdrawn_mirror_asset_amount =
                        info.terraswap_pool_mirror_asset_amount * fraction;
                    let withdrawn_uusd_amount = info.terraswap_pool_uusd_amount * fraction;
                    let new_lp_total_supply = info.lp_token_total_supply - withdraw_lp_token_amount;
                    let pool_mirror_asset_amount_after_withdrawal =
                        info.terraswap_pool_mirror_asset_amount - withdrawn_mirror_asset_amount;
                    let pool_uusd_amount_after_withdrawal =
                        info.terraswap_pool_uusd_amount - withdrawn_uusd_amount;
                    let (_, pool_mirror_asset_amount_after_swap, return_mirror_asset_amount) =
                        simulate_terraswap_swap(
                            pool_uusd_amount_after_withdrawal,
                            pool_mirror_asset_amount_after_withdrawal,
                            withdrawn_uusd_amount + state.uusd_balance,
                        );
                    let mirror_asset_long_farm_amount = pool_mirror_asset_amount_after_swap
                        * Decimal::from_ratio(
                            info.lp_token_amount - withdraw_lp_token_amount,
                            new_lp_total_supply,
                        );
                    let mirror_asset_long_amount = state.mirror_asset_balance
                        + withdrawn_mirror_asset_amount
                        + return_mirror_asset_amount
                        + mirror_asset_long_farm_amount;
                    if mirror_asset_long_amount <= state.mirror_asset_short_amount {
                        a = withdraw_lp_token_amount;
                    } else {
                        b = withdraw_lp_token_amount;
                    }
                }

                let offer_uusd_amount = info.terraswap_pool_uusd_amount
                    * Decimal::from_ratio(a, info.lp_token_total_supply)
                    + state.uusd_balance;
                response = response
                    .add_messages(unstake_lp_and_withdraw_liquidity(
                        &state,
                        &context,
                        &mirror_asset_cw20_addr,
                        a,
                    ))
                    .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: info.terraswap_pair_addr.clone(),
                        msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                            offer_asset: get_uusd_asset_from_amount(offer_uusd_amount),
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })?,
                        funds: vec![Coin {
                            denom: String::from("uusd"),
                            amount: offer_uusd_amount,
                        }],
                    }));
            } else {
                let mut b = state.uusd_balance + one;
                while b > a + one {
                    let offer_uusd_amount = (a + b) >> 1;
                    let (_, pool_mirror_asset_amount_after_swap, return_mirror_asset_amount) =
                        simulate_terraswap_swap(
                            info.terraswap_pool_uusd_amount,
                            info.terraswap_pool_mirror_asset_amount,
                            offer_uusd_amount,
                        );
                    let mirror_asset_long_amount = pool_mirror_asset_amount_after_swap
                        * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply)
                        + return_mirror_asset_amount
                        + state.mirror_asset_balance;
                    if mirror_asset_long_amount <= state.mirror_asset_short_amount {
                        a = offer_uusd_amount;
                    } else {
                        b = offer_uusd_amount;
                    }
                }
                response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: info.terraswap_pair_addr.clone(),
                    msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                        offer_asset: get_uusd_asset_from_amount(a),
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    })?,
                    funds: vec![Coin {
                        denom: String::from("uusd"),
                        amount: a,
                    }],
                }));
            }
        }
        Ordering::Equal => {}
    }
    Ok(response)
}

pub fn achieve_safe_collateral_ratios(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let collateral_ratio = Decimal::from_ratio(
        state.collateral_uusd_value,
        state.mirror_asset_oracle_price * state.mirror_asset_short_amount,
    );
    let target_collateral_ratio_range = TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?;
    let mut response = Response::new();
    if collateral_ratio < target_collateral_ratio_range.min {
        let target_short_mirror_asset_amount = state.collateral_uusd_value
            * reverse_decimal(decimal_multiplication(
                target_collateral_ratio_range.midpoint(),
                state.mirror_asset_oracle_price,
            ));

        // Burn mAsset against the short position.
        let burn_mirror_asset_amount =
            state.mirror_asset_short_amount - target_short_mirror_asset_amount;
        response = response.add_messages(increase_mirror_asset_balance_from_long_farm(
            &state,
            &context,
            &MIRROR_ASSET_CW20_ADDR.load(deps.storage)?,
            burn_mirror_asset_amount,
        ));
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: MIRROR_ASSET_CW20_ADDR.load(deps.storage)?.to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: burn_mirror_asset_amount,
                msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                    position_idx: CDP_IDX.load(deps.storage)?,
                })?,
            })?,
        }));
    } else if collateral_ratio > target_collateral_ratio_range.max {
        let target_anchor_ust_collateral_amount = state.mirror_asset_short_amount
            * state.mirror_asset_oracle_price
            * decimal_division(
                target_collateral_ratio_range.midpoint(),
                state.anchor_ust_oracle_price,
            );

        // Withdraw aUST collateral and redeem for UST.
        let withdraw_anchor_ust_collateral_amount =
            state.collateral_anchor_ust_amount - target_anchor_ust_collateral_amount;
        response = response.add_messages(increase_uusd_balance_from_aust_collateral(
            &context,
            CDP_IDX.load(deps.storage)?,
            withdraw_anchor_ust_collateral_amount,
        ));
    }
    Ok(response)
}

pub fn open_position(
    deps: DepsMut,
    env: Env,
    context: Context,
    target_min_collateral_ratio: Decimal,
    target_max_collateral_ratio: Decimal,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    if CDP_IDX.load(deps.storage).is_ok() {
        return Err(StdError::generic_err("position is already open"));
    }

    let uusd_balance = get_uusd_balance(&deps.querier, &env)?;
    if uusd_balance < context.min_delta_neutral_uusd_amount {
        return Err(StdError::generic_err(
            "UST amount too small to open a delta-neutral position",
        ));
    }

    POSITION_OPEN_INFO.save(
        deps.storage,
        &PositionActionInfo {
            height: env.block.height,
            time_nanoseconds: env.block.time.nanos(),
            uusd_amount: uusd_balance,
        },
    )?;

    let mirror_asset_cw20_addr = deps.api.addr_validate(&mirror_asset_cw20_addr)?;
    MIRROR_ASSET_CW20_ADDR.save(deps.storage, &mirror_asset_cw20_addr)?;

    let cdp_idx_response: mirror_protocol::mint::NextPositionIdxResponse =
        deps.querier.query_wasm_smart(
            context.mirror_mint_addr.clone(),
            &mirror_protocol::mint::QueryMsg::NextPositionIdx {},
        )?;
    CDP_IDX.save(deps.storage, &cdp_idx_response.next_position_idx)?;

    TARGET_COLLATERAL_RATIO_RANGE.save(
        deps.storage,
        &TargetCollateralRatioRange {
            min: target_min_collateral_ratio,
            max: target_max_collateral_ratio,
        },
    )?;
    delta_neutral_invest(deps, env, context, &mirror_asset_cw20_addr, None)
}

pub fn delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    context: Context,
    mirror_asset_cw20_addr: &Addr,
    cdp_idx: Option<Uint128>,
) -> StdResult<Response> {
    let uusd_balance = get_uusd_balance(&deps.querier, &env)?;
    if uusd_balance < context.min_delta_neutral_uusd_amount {
        return Ok(Response::default());
    }
    let target_collateral_ratio_range = TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?;
    find_collateral_uusd_amount(
        deps.as_ref(),
        &env,
        &context,
        mirror_asset_cw20_addr,
        &target_collateral_ratio_range,
        uusd_balance,
        cdp_idx,
    )
}

pub fn decrease_position(
    env: Env,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    Ok(Response::new().add_messages(vec![
        create_internal_execute_message(&env, InternalExecuteMsg::ClaimAndIncreaseUusdBalance {}),
        create_internal_execute_message(&env, InternalExecuteMsg::AchieveDeltaNeutral {}),
        create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawFundsInUusd {
                proportion,
                recipient,
            },
        ),
    ]))
}

pub fn send_uusd_to_recipient(
    deps: DepsMut,
    env: Env,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    let amount = get_uusd_balance(&deps.querier, &env)? * proportion;

    // Record POSITION_CLOSE_INFO if the position is being closed.
    if proportion == Decimal::one() {
        POSITION_CLOSE_INFO.save(
            deps.storage,
            &PositionActionInfo {
                height: env.block.height,
                time_nanoseconds: env.block.time.nanos(),
                uusd_amount: amount,
            },
        )?;
    }

    if amount.is_zero() {
        return Ok(Response::default());
    }

    // Initiate transfer of `amount` uusd to the recipient.
    let position_manager_admin_config: delta_neutral_position_manager::AdminConfig =
        deps.querier.query_wasm_smart(
            MANAGER.load(deps.storage)?,
            &delta_neutral_position_manager::QueryMsg::GetAdminConfig {},
        )?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: position_manager_admin_config.terra_manager.to_string(),
            msg: to_binary(&terra_manager::ExecuteMsg::InitiateOutgoingTokenTransfer {
                assets: vec![get_uusd_asset_from_amount(amount)],
                recipient,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount,
            }],
        })),
    )
}

pub fn withdraw_collateral_and_redeem_for_uusd(
    deps: Deps,
    context: Context,
    proportion: Decimal,
) -> StdResult<Response> {
    let cdp_idx = CDP_IDX.load(deps.storage)?;
    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Position {
                position_idx: cdp_idx,
            },
        )?;
    Ok(
        Response::new().add_messages(increase_uusd_balance_from_aust_collateral(
            &context,
            cdp_idx,
            position_response.collateral.amount * proportion,
        )),
    )
}

pub fn withdraw_funds_in_uusd(
    deps: Deps,
    env: Env,
    context: Context,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;

    let mut response = Response::new();

    // Reduce mAsset short position.
    let mirror_asset_burn_amount = state.mirror_asset_short_amount * proportion;
    response = response.add_messages(increase_mirror_asset_balance_from_long_farm(
        &state,
        &context,
        &mirror_asset_cw20_addr,
        mirror_asset_burn_amount,
    ));
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: context.mirror_mint_addr.to_string(),
            amount: mirror_asset_burn_amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: CDP_IDX.load(deps.storage)?,
            })?,
        })?,
        funds: vec![],
    }));

    // Send uusd to recipient.
    response = response.add_messages(vec![
        create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawCollateralAndRedeemForUusd { proportion },
        ),
        create_internal_execute_message(
            &env,
            InternalExecuteMsg::SendUusdToRecipient {
                proportion,
                recipient,
            },
        ),
    ]);

    Ok(response)
}

pub fn stake_terraswap_lp_tokens(
    deps: Deps,
    env: Env,
    context: Context,
    lp_token_cw20_addr: String,
) -> StdResult<Response> {
    let lp_token_amount = terraswap::querier::query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&lp_token_cw20_addr)?,
        env.contract.address,
    )?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_cw20_addr,
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.spectrum_mirror_farms_addr.to_string(),
                amount: lp_token_amount,
                msg: to_binary(&spectrum_protocol::mirror_farm::Cw20HookMsg::bond {
                    asset_token: MIRROR_ASSET_CW20_ADDR.load(deps.storage)?.to_string(),
                    compound_rate: Some(Decimal::one()),
                    staker_addr: None,
                })?,
            })?,
            funds: vec![],
        })),
    )
}

pub fn pair_uusd_with_mirror_asset_to_provide_liquidity_and_stake(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
    let mut response = Response::new();
    if state.mirror_asset_balance.is_zero() || state.uusd_balance.is_zero() {
        return Ok(response);
    }

    // Find amount of uusd and mAsset to pair together and provide liquidity.
    let (terraswap_pair_info, pool_mirror_asset_balance, pool_uusd_balance) =
        get_terraswap_mirror_asset_uusd_liquidity_info(
            deps,
            &context.terraswap_factory_addr,
            &mirror_asset_cw20_addr,
        )?;
    let uusd_ratio = Decimal::from_ratio(state.uusd_balance, pool_uusd_balance);
    let mirror_asset_ratio =
        Decimal::from_ratio(state.mirror_asset_balance, pool_mirror_asset_balance);
    let ratio = min(uusd_ratio, mirror_asset_ratio);
    let uusd_provide_amount = pool_uusd_balance * ratio;
    let mirror_asset_provide_amount = pool_mirror_asset_balance * ratio;

    // Allow Terraswap mAsset-UST pair contract to transfer mAsset tokens from us.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pair_info.contract_addr.clone(),
            amount: mirror_asset_provide_amount,
            expires: None,
        })?,
        funds: vec![],
    }));

    // Provide liquidity to Terraswap mAsset-UST pool.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair_info.contract_addr,
        msg: to_binary(&terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: mirror_asset_cw20_addr.to_string(),
                    },
                    amount: mirror_asset_provide_amount,
                },
                get_uusd_asset_from_amount(uusd_provide_amount),
            ],
            slippage_tolerance: None,
            receiver: None,
        })?,
        funds: vec![Coin {
            denom: String::from("uusd"),
            amount: uusd_provide_amount,
        }],
    }));

    // Stake Terraswap LP tokens at Spectrum Mirror Vault.
    Ok(response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::StakeTerraswapLpTokens {
            lp_token_cw20_addr: terraswap_pair_info.liquidity_token,
        },
    )))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let manager_addr = MANAGER.load(deps.storage)?;
    let context: Context = deps
        .querier
        .query_wasm_smart(&manager_addr, &ManagerQueryMsg::GetContext {})?;
    match msg {
        QueryMsg::GetPositionInfo {} => to_binary(&query_position_info(deps, &env, &context)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
