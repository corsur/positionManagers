use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::state::{
    PositionInfo, TargetCollateralRatioRange, MANAGER, POSITION_INFO, TARGET_COLLATERAL_RATIO_RANGE,
};
use crate::util::{
    create_terraswap_cw20_uusd_pair_asset_info, decimal_division, decimal_inverse,
    decimal_multiplication, find_collateral_uusd_amount, get_mirror_asset_oracle_uusd_price,
    get_position_state, get_uusd_asset_from_amount, increase_mirror_asset_balance_from_long_farm,
    increase_uusd_balance_from_aust_collateral, increase_uusd_balance_from_long_farm,
    swap_cw20_token_for_uusd,
};
use aperture_common::delta_neutral_position::{
    ControllerExecuteMsg, ExecuteMsg, InstantiateMsg, InternalExecuteMsg, MigrateMsg, QueryMsg,
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
    let context: Context = deps
        .querier
        .query_wasm_smart(&manager_addr, &ManagerQueryMsg::GetContext {})?;
    let is_authorized = match msg {
        ExecuteMsg::Controller(_) => {
            info.sender == context.controller || info.sender == env.contract.address
        }
        ExecuteMsg::Internal(_) => info.sender == env.contract.address,
        _ => info.sender == manager_addr,
    };
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "unauthorized".to_string(),
        });
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
        ExecuteMsg::IncreasePosition {} => rebalance_and_reinvest(env),
        ExecuteMsg::DecreasePosition {
            proportion,
            recipient,
        } => decrease_position(env, proportion, recipient),
        ExecuteMsg::Controller(controller_msg) => match controller_msg {
            ControllerExecuteMsg::RebalanceAndReinvest {} => rebalance_and_reinvest(env),
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
            InternalExecuteMsg::WithdrawUusd {
                proportion,
                recipient,
            } => withdraw_uusd(deps.as_ref(), env, proportion, recipient),
            InternalExecuteMsg::DepositUusdBalanceToAnchor {} => {
                deposit_uusd_balance_to_anchor(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::AddAnchorUstBalanceToCollateral {} => {
                add_anchor_ust_balance_to_collateral(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::OpenOrIncreaseCdpWithAnchorUstBalanceAsCollateral {
                collateral_ratio,
                mirror_asset_cw20_addr,
                cdp_idx,
                mirror_asset_mint_amount,
            } => open_or_increase_cdp_with_anchor_ust_balance_as_collateral(
                deps.as_ref(),
                env,
                context,
                collateral_ratio,
                mirror_asset_cw20_addr,
                cdp_idx,
                mirror_asset_mint_amount,
            ),
            InternalExecuteMsg::RecordPositionInfo {
                mirror_asset_cw20_addr,
            } => record_position_info(deps, env, context, mirror_asset_cw20_addr),
            InternalExecuteMsg::StakeTerraswapLpTokens { lp_token_cw20_addr } => {
                stake_terraswap_lp_tokens(deps.as_ref(), env, context, lp_token_cw20_addr)
            }
        },
    }
}

pub fn deposit_uusd_balance_to_anchor(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let uusd_asset = Asset {
        amount: terraswap::querier::query_balance(
            &deps.querier,
            env.contract.address,
            String::from("uusd"),
        )?,
        info: AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    };
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![uusd_asset.deduct_tax(&deps.querier)?],
        })),
    )
}

pub fn add_anchor_ust_balance_to_collateral(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let aust_amount = terraswap::querier::query_token_balance(
        &deps.querier,
        context.anchor_ust_cw20_addr.clone(),
        env.contract.address,
    )?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: aust_amount,
                msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Deposit {
                    position_idx: POSITION_INFO.load(deps.storage)?.cdp_idx,
                })?,
            })?,
            funds: vec![],
        })),
    )
}

fn create_internal_execute_message(env: &Env, msg: InternalExecuteMsg) -> CosmosMsg {
    CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::Internal(msg)).unwrap(),
        funds: vec![],
    })
}

pub fn get_rebalance_internal_messages(env: &Env) -> Vec<CosmosMsg> {
    vec![
        create_internal_execute_message(env, InternalExecuteMsg::ClaimAndIncreaseUusdBalance {}),
        create_internal_execute_message(env, InternalExecuteMsg::AchieveDeltaNeutral {}),
        create_internal_execute_message(env, InternalExecuteMsg::AchieveSafeCollateralRatio {}),
    ]
}

pub fn get_reinvest_internal_messages(_env: &Env) -> Vec<CosmosMsg> {
    // TODO:
    // (1) If possible, pair UST with mAsset balance and stake LP in Spectrum.
    // (2) If uusd balance at this point is nontrivial and there is no pending short proceeds, increase delta-neutral position with uusd balance.
    vec![]
}

pub fn rebalance_and_reinvest(env: Env) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(get_rebalance_internal_messages(&env))
        .add_messages(get_reinvest_internal_messages(&env)))
}

pub fn claim_and_increase_uusd_balance(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    // Find claimable SPEC reward.
    let spec_reward_info_response: spectrum_protocol::mirror_farm::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            &context.spectrum_mirror_farms_addr,
            &spectrum_protocol::mirror_farm::QueryMsg::reward_info {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            },
        )?;
    let mut spec_reward = Uint128::zero();
    for reward_info in spec_reward_info_response.reward_infos.iter() {
        spec_reward += reward_info.pending_spec_reward;
    }

    // Find claimable MIR reward.
    let mir_reward_info_response: mirror_protocol::staking::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_staking_addr,
            &mirror_protocol::staking::QueryMsg::RewardInfo {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            },
        )?;
    let mut mir_reward = Uint128::zero();
    for reward_info in mir_reward_info_response.reward_infos.iter() {
        mir_reward += reward_info.pending_reward;
    }

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
            context.terraswap_factory_addr.clone(),
            context.spectrum_cw20_addr.as_str(),
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
            context.terraswap_factory_addr,
            context.mirror_cw20_addr.as_str(),
            mir_reward,
        )?);
    }

    // If there are any unlocked funds in the short farm, claim them.
    let position_info = POSITION_INFO.load(deps.storage)?;
    let position_lock_info_result: StdResult<mirror_protocol::lock::PositionLockInfoResponse> =
        deps.querier.query_wasm_smart(
            &context.mirror_lock_addr,
            &mirror_protocol::lock::QueryMsg::PositionLockInfo {
                position_idx: position_info.cdp_idx,
            },
        );
    if let Ok(position_lock_info_response) = position_lock_info_result {
        if position_lock_info_response.unlock_time <= env.block.time.seconds() {
            response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.mirror_lock_addr.to_string(),
                msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
                    positions_idx: vec![position_info.cdp_idx],
                })?,
                funds: vec![],
            }));
        }
    }

    Ok(response)
}

pub fn achieve_delta_neutral(deps: Deps, env: Env, context: Context) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let mirror_asset_long_amount = state.mirror_asset_balance + state.mirror_asset_long_farm;
    let mut response = Response::new();
    if mirror_asset_long_amount > state.mirror_asset_short_amount {
        // We are in a net long position, so we swap the difference, i.e. `offer_mirror_asset_amount` for UST.
        let net_long_mirror_asset_amount =
            mirror_asset_long_amount - state.mirror_asset_short_amount;
        response = response.add_messages(increase_mirror_asset_balance_from_long_farm(
            &state,
            &context,
            net_long_mirror_asset_amount,
        ));
        response = response.add_message(swap_cw20_token_for_uusd(
            &deps.querier,
            context.terraswap_factory_addr,
            state.mirror_asset_cw20_addr.as_str(),
            net_long_mirror_asset_amount,
        )?);
    } else if mirror_asset_long_amount < state.mirror_asset_short_amount {
        // We are in a net short position, so we swap uusd for the difference amount of mAsset.
        let net_short_mirror_asset_amount =
            state.mirror_asset_short_amount - mirror_asset_long_amount;
        let terraswap_pair_addr = deps.api.addr_validate(
            &terraswap::querier::query_pair_info(
                &deps.querier,
                context.terraswap_factory_addr.clone(),
                &create_terraswap_cw20_uusd_pair_asset_info(state.mirror_asset_cw20_addr.as_str()),
            )?
            .contract_addr,
        )?;
        let uusd_offer_amount = terraswap::querier::reverse_simulate(
            &deps.querier,
            terraswap_pair_addr.clone(),
            &Asset {
                info: AssetInfo::Token {
                    contract_addr: state.mirror_asset_cw20_addr.to_string(),
                },
                amount: net_short_mirror_asset_amount,
            },
        )?
        .offer_amount;

        // If uusd balance is insufficient to cover `uuse_offer_amount` + tax, then increase uusd balance by unstaking LPs and withdrawing liquidity.
        let uusd_offer_asset = get_uusd_asset_from_amount(uusd_offer_amount);
        response = response
            .add_messages(increase_uusd_balance_from_long_farm(
                &deps.querier,
                &state,
                &context,
                uusd_offer_amount + uusd_offer_asset.compute_tax(&deps.querier)?,
            ))
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: terraswap_pair_addr.to_string(),
                msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                    offer_asset: get_uusd_asset_from_amount(uusd_offer_amount),
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
                funds: vec![Coin {
                    denom: String::from("uusd"),
                    amount: uusd_offer_amount,
                }],
            }));
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
        get_mirror_asset_oracle_uusd_price(
            &deps.querier,
            &context,
            state.mirror_asset_cw20_addr.as_str(),
        )? * state.mirror_asset_short_amount,
    );
    let target_collateral_ratio_range = TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?;
    let mut response = Response::new();
    if collateral_ratio < target_collateral_ratio_range.min {
        let target_short_mirror_asset_amount = state.collateral_uusd_value
            * decimal_inverse(decimal_multiplication(
                target_collateral_ratio_range.midpoint(),
                state.mirror_asset_oracle_price,
            ));

        // Burn mAsset against the short position.
        let burn_mirror_asset_amount =
            state.mirror_asset_short_amount - target_short_mirror_asset_amount;
        response = response.add_messages(increase_mirror_asset_balance_from_long_farm(
            &state,
            &context,
            burn_mirror_asset_amount,
        ));
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.mirror_asset_cw20_addr.to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: burn_mirror_asset_amount,
                msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                    position_idx: POSITION_INFO.load(deps.storage)?.cdp_idx,
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
            POSITION_INFO.load(deps.storage)?.cdp_idx,
            withdraw_anchor_ust_collateral_amount,
        ));
    }
    Ok(response)
}

fn get_cdp_index(deps: Deps, env: Env, context: &Context) -> StdResult<Uint128> {
    let positions_response: mirror_protocol::mint::PositionsResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Positions {
                owner_addr: Some(env.contract.address.to_string()),
                asset_token: None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )?;
    Ok(positions_response.positions[0].idx)
}

pub fn open_position(
    deps: DepsMut,
    env: Env,
    context: Context,
    target_min_collateral_ratio: Decimal,
    target_max_collateral_ratio: Decimal,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    if POSITION_INFO.load(deps.storage).is_ok() {
        return Err(StdError::GenericErr {
            msg: "delta_neutral_position_already_exists".to_string(),
        });
    }
    let target_collateral_ratio_range = TargetCollateralRatioRange {
        min: target_min_collateral_ratio,
        max: target_max_collateral_ratio,
    };
    TARGET_COLLATERAL_RATIO_RANGE.save(deps.storage, &target_collateral_ratio_range)?;
    delta_neutral_invest(deps, env, context, mirror_asset_cw20_addr, None)
}

pub fn delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    context: Context,
    mirror_asset_cw20_addr: String,
    cdp_idx: Option<Uint128>,
) -> StdResult<Response> {
    let uusd_balance = terraswap::querier::query_balance(
        &deps.querier,
        env.contract.address.clone(),
        String::from("uusd"),
    )?;
    let target_collateral_ratio_range = TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?;
    let (mirror_asset_mint_amount, collateral_uusd_amount) = find_collateral_uusd_amount(
        deps.as_ref(),
        &context,
        &mirror_asset_cw20_addr,
        &target_collateral_ratio_range,
        uusd_balance,
    )?;
    Ok(Response::new().add_messages(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: collateral_uusd_amount,
            }],
        }),
        create_internal_execute_message(
            &env,
            InternalExecuteMsg::OpenOrIncreaseCdpWithAnchorUstBalanceAsCollateral {
                collateral_ratio: target_collateral_ratio_range.midpoint(),
                mirror_asset_cw20_addr: mirror_asset_cw20_addr.clone(),
                cdp_idx,
                mirror_asset_mint_amount,
            },
        ),
        create_internal_execute_message(
            &env,
            InternalExecuteMsg::RecordPositionInfo {
                mirror_asset_cw20_addr,
            },
        ),
        create_internal_execute_message(&env, InternalExecuteMsg::AchieveDeltaNeutral {}),
    ]))
}

fn open_or_increase_cdp_with_anchor_ust_balance_as_collateral(
    deps: Deps,
    env: Env,
    context: Context,
    collateral_ratio: Decimal,
    mirror_asset_cw20_addr: String,
    cdp_idx: Option<Uint128>,
    mirror_asset_mint_amount: Uint128,
) -> StdResult<Response> {
    let anchor_ust_balance = terraswap::querier::query_token_balance(
        &deps.querier,
        context.anchor_ust_cw20_addr.clone(),
        env.contract.address,
    )?;
    match cdp_idx {
        None => Ok(
            Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.anchor_ust_cw20_addr.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: context.mirror_mint_addr.to_string(),
                    amount: anchor_ust_balance,
                    msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                        asset_info: AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr,
                        },
                        collateral_ratio,
                        short_params: Some(mirror_protocol::mint::ShortParams {
                            belief_price: None,
                            max_spread: None,
                        }),
                    })?,
                })?,
                funds: vec![],
            })),
        ),
        Some(position_idx) => Ok(Response::new()
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.anchor_ust_cw20_addr.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: context.mirror_mint_addr.to_string(),
                    amount: anchor_ust_balance,
                    msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Deposit { position_idx })?,
                })?,
                funds: vec![],
            }))
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.mirror_mint_addr.to_string(),
                msg: to_binary(&mirror_protocol::mint::ExecuteMsg::Mint {
                    position_idx,
                    asset: Asset {
                        info: AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr,
                        },
                        amount: mirror_asset_mint_amount,
                    },
                    short_params: Some(mirror_protocol::mint::ShortParams {
                        belief_price: None,
                        max_spread: None,
                    }),
                })?,
                funds: vec![],
            }))),
    }
}

fn record_position_info(
    deps: DepsMut,
    env: Env,
    context: Context,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    let position_info = PositionInfo {
        cdp_idx: get_cdp_index(deps.as_ref(), env, &context)?,
        mirror_asset_cw20_addr: deps.api.addr_validate(&mirror_asset_cw20_addr)?,
    };
    POSITION_INFO.save(deps.storage, &position_info)?;
    Ok(Response::default())
}

pub fn decrease_position(env: Env, proportion: Decimal, recipient: String) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(get_rebalance_internal_messages(&env))
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawFundsInUusd {
                proportion,
                recipient,
            },
        )))
}

pub fn withdraw_uusd(
    deps: Deps,
    env: Env,
    proportion: Decimal,
    recipient: String,
) -> StdResult<Response> {
    let amount =
        terraswap::querier::query_balance(&deps.querier, env.contract.address, "uusd".into())?
            * proportion;
    if amount == Uint128::zero() {
        return Ok(Response::default());
    }
    Ok(Response::new().add_message(CosmosMsg::Bank(BankMsg::Send {
        to_address: recipient,
        amount: vec![get_uusd_asset_from_amount(amount).deduct_tax(&deps.querier)?],
    })))
}

pub fn withdraw_funds_in_uusd(
    deps: Deps,
    env: Env,
    context: Context,
    proportion: Decimal,
    recipient: String,
) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let cdp_idx = POSITION_INFO.load(deps.storage)?.cdp_idx;

    let mut response = Response::new();

    // Reduce mAsset short position.
    let mirror_asset_burn_amount = state.mirror_asset_short_amount * proportion;
    response = response.add_messages(increase_mirror_asset_balance_from_long_farm(
        &state,
        &context,
        mirror_asset_burn_amount,
    ));
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: state.mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: context.mirror_mint_addr.to_string(),
            amount: mirror_asset_burn_amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: cdp_idx,
            })?,
        })?,
        funds: vec![],
    }));

    // Withdraw aUST collateral and redeem for UST.
    response = response.add_messages(increase_uusd_balance_from_aust_collateral(
        &context,
        cdp_idx,
        state.collateral_anchor_ust_amount * proportion,
    ));

    // Send uusd to recipient.
    response = response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::WithdrawUusd {
            proportion,
            recipient,
        },
    ));

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
    let position_info = POSITION_INFO.load(deps.storage)?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token_cw20_addr,
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.spectrum_mirror_farms_addr.to_string(),
                amount: lp_token_amount,
                msg: to_binary(&spectrum_protocol::mirror_farm::Cw20HookMsg::bond {
                    asset_token: position_info.mirror_asset_cw20_addr.to_string(),
                    compound_rate: Some(Decimal::one()),
                    staker_addr: None,
                })?,
            })?,
            funds: vec![],
        })),
    )
}

pub fn pair_ust_with_mirror_asset_and_stake(
    deps: Deps,
    env: Env,
    context: Context,
    mirror_asset_amount: Uint128,
) -> StdResult<Response> {
    let position_info = POSITION_INFO.load(deps.storage)?;
    let mut response = Response::new();

    // Find uusd amount to pair with mAsset of quantity `mirror_asset_amount`.
    let terraswap_pair_asset_info = create_terraswap_cw20_uusd_pair_asset_info(
        &position_info.mirror_asset_cw20_addr.to_string(),
    );
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr,
        &terraswap_pair_asset_info,
    )?;
    let terraswap_pair_contract_addr =
        deps.api.addr_validate(&terraswap_pair_info.contract_addr)?;
    let pool_mirror_asset_balance = terraswap_pair_asset_info[0].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr.clone(),
    )?;
    let pool_uusd_balance = terraswap_pair_asset_info[1].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr,
    )?;
    let uusd_amount_to_provide_liquidity =
        mirror_asset_amount.multiply_ratio(pool_uusd_balance, pool_mirror_asset_balance);

    // Allow Terraswap mAsset-UST pair contract to transfer mAsset tokens from us.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pair_info.contract_addr.clone(),
            amount: mirror_asset_amount,
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
                        contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
                    },
                    amount: mirror_asset_amount,
                },
                get_uusd_asset_from_amount(uusd_amount_to_provide_liquidity),
            ],
            slippage_tolerance: None,
            receiver: None,
        })?,
        funds: vec![Coin {
            denom: String::from("uusd"),
            amount: uusd_amount_to_provide_liquidity,
        }],
    }));

    // Stake Terraswap LP tokens to Mirror Long Farm or Spectrum Mirror Vault.
    Ok(response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::StakeTerraswapLpTokens {
            lp_token_cw20_addr: terraswap_pair_info.liquidity_token,
        },
    )))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo {} => to_binary(&(POSITION_INFO.load(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
