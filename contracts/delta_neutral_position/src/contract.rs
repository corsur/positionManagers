use std::cmp::min;

use aperture_common::common::Recipient;
use aperture_common::delta_neutral_position_manager::{self, Context};
use aperture_common::terra_manager;
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, WasmMsg,
};
use cw_storage_plus::Item;
use terraswap::asset::{Asset, AssetInfo};

use crate::dex_util::get_terraswap_mirror_asset_uusd_liquidity_info;
use crate::math::{decimal_division, decimal_multiplication, reverse_decimal};
use crate::open::delta_neutral_invest;
use crate::rebalance::achieve_delta_neutral;
use crate::state::{
    CDP_IDX, MANAGER, MIRROR_ASSET_CW20_ADDR, POSITION_CLOSE_INFO, POSITION_OPEN_INFO,
    TARGET_COLLATERAL_RATIO_RANGE,
};
use crate::util::{
    get_cdp_uusd_lock_info_result, get_position_state, get_uusd_asset_from_amount,
    get_uusd_balance, increase_mirror_asset_balance_from_long_farm,
    increase_uusd_balance_from_aust_collateral, query_position_info,
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
        } => decrease_position(deps.as_ref(), env, context, proportion, recipient),
        ExecuteMsg::Controller(controller_msg) => match controller_msg {
            ControllerExecuteMsg::RebalanceAndReinvest {} => {
                rebalance_and_reinvest(deps.as_ref(), env, context)
            }
        },
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
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
                let target_collateral_ratio_range =
                    TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?;
                let uusd_amount = get_uusd_balance(&deps.querier, &env)?;
                if uusd_amount >= context.min_reinvest_uusd_amount {
                    delta_neutral_invest(
                        deps,
                        env,
                        context,
                        uusd_amount,
                        &target_collateral_ratio_range,
                        &mirror_asset_cw20_addr,
                        Some(cdp_idx),
                    )
                } else {
                    Ok(Response::default())
                }
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
        .add_messages(achieve_delta_neutral(deps, &env, &context)?)
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::AchieveSafeCollateralRatio {},
        ))
        .add_messages(get_reinvest_internal_messages(deps, &env, &context)))
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
    if uusd_balance < context.min_open_uusd_amount {
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

    let target_collateral_ratio_range = TargetCollateralRatioRange {
        min: target_min_collateral_ratio,
        max: target_max_collateral_ratio,
    };
    TARGET_COLLATERAL_RATIO_RANGE.save(deps.storage, &target_collateral_ratio_range)?;
    delta_neutral_invest(
        deps,
        env,
        context,
        uusd_balance,
        &target_collateral_ratio_range,
        &mirror_asset_cw20_addr,
        None,
    )
}

pub fn decrease_position(
    deps: Deps,
    env: Env,
    context: Context,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(achieve_delta_neutral(deps, &env, &context)?)
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawFundsInUusd {
                proportion,
                recipient,
            },
        )))
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
    response = response
        .add_messages(increase_mirror_asset_balance_from_long_farm(
            &state,
            &context,
            &mirror_asset_cw20_addr,
            mirror_asset_burn_amount,
        ))
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
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
