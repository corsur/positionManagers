use std::cmp::min;

use aperture_common::common::Recipient;
use aperture_common::delta_neutral_position_manager::{self, Context, FeeCollectionConfig};
use aperture_common::terra_manager;
use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::Item;
use terraswap::asset::{Asset, AssetInfo};

use crate::math::{decimal_division, decimal_multiplication, reverse_decimal};
use crate::open::delta_neutral_invest;
use crate::rebalance::achieve_delta_neutral;
use crate::spectrum_util::check_spectrum_mirror_farm_existence;
use crate::state::{
    CDP_IDX, LAST_FEE_COLLECTION_POSITION_UUSD_VALUE, MANAGER, MIRROR_ASSET_CW20_ADDR,
    POSITION_CLOSE_INFO, POSITION_OPEN_INFO, TARGET_COLLATERAL_RATIO_RANGE,
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
        ExecuteMsg::ClosePosition { recipient } => {
            close_position(deps.as_ref(), env, context, recipient)
        }
        ExecuteMsg::Controller(controller_msg) => match controller_msg {
            ControllerExecuteMsg::RebalanceAndReinvest {} => {
                rebalance_and_reinvest(deps.as_ref(), env, context)
            }
            ControllerExecuteMsg::RebalanceAndCollectFees {} => {
                rebalance_and_collect_fees(deps.as_ref(), env, context)
            }
        },
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
            InternalExecuteMsg::AchieveSafeCollateralRatio {} => {
                achieve_safe_collateral_ratios(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::WithdrawFundsInUusd { recipient } => {
                withdraw_funds_in_uusd(deps, env, context, recipient)
            }
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
    // If there is still short proceeds pending unlock, we don't reinvest as this could reset the locking period.
    if let Ok(lock_info_response) = get_cdp_uusd_lock_info_result(deps, context) {
        if !lock_info_response.locked_amount.is_zero()
            && lock_info_response.unlock_time > env.block.time.seconds()
        {
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

pub fn rebalance_and_collect_fees(deps: Deps, env: Env, context: Context) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(achieve_delta_neutral(deps, &env, &context)?)
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawFundsInUusd { recipient: None },
        )))
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
            &context.spectrum_mirror_farms_addr,
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

    LAST_FEE_COLLECTION_POSITION_UUSD_VALUE.save(deps.storage, &uusd_balance)?;
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

pub fn close_position(
    deps: Deps,
    env: Env,
    context: Context,
    recipient: Recipient,
) -> StdResult<Response> {
    Ok(Response::new()
        .add_messages(achieve_delta_neutral(deps, &env, &context)?)
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::WithdrawFundsInUusd {
                recipient: Some(recipient),
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

// Reduce position for protocol fee collection and/or position close.
// If `recipient` is None, then only protocol fees are collected;
// otherwise, this closes the position, and will send the remaining amount to `recipient`.
pub fn withdraw_funds_in_uusd(
    deps: DepsMut,
    env: Env,
    context: Context,
    recipient: Option<Recipient>,
) -> StdResult<Response> {
    let state = get_position_state(deps.as_ref(), &env, &context)?;
    let position_value = state.collateral_uusd_value + state.uusd_balance + state.uusd_long_farm;
    let last_fee_collection_position_uusd_value =
        LAST_FEE_COLLECTION_POSITION_UUSD_VALUE.load(deps.storage)?;
    let gain = if last_fee_collection_position_uusd_value < position_value {
        position_value - last_fee_collection_position_uusd_value
    } else {
        Uint128::zero()
    };
    let manager_addr = MANAGER.load(deps.storage)?;
    const FEE_COLLECTION_CONFIG: Item<FeeCollectionConfig> = Item::new("fee_collection_config");
    let fee_collection_config = FEE_COLLECTION_CONFIG.query(&deps.querier, manager_addr)?;
    let fee_amount = gain * fee_collection_config.performance_rate;
    let fee_proportion = Decimal::from_ratio(fee_amount, position_value);

    let proportion = if recipient.is_some() {
        Decimal::one()
    } else {
        fee_proportion
    };
    if proportion.is_zero() {
        return Ok(Response::default());
    }
    let mut response = Response::new();

    // Reduce mAsset short position.
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
    let mirror_asset_burn_amount = state.mirror_asset_short_amount * proportion;
    response = response
        .add_messages(increase_mirror_asset_balance_from_long_farm(
            &state,
            &context.spectrum_mirror_farms_addr,
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

    // Withdraw aUST collateral and redeem for uusd.
    response = response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::WithdrawCollateralAndRedeemForUusd { proportion },
    ));

    // Send protocol fees to fee collector.
    if !fee_amount.is_zero() {
        response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
            to_address: fee_collection_config.collector_addr,
            amount: vec![Coin {
                denom: String::from("uusd"),
                amount: fee_amount,
            }],
        }));
        LAST_FEE_COLLECTION_POSITION_UUSD_VALUE
            .save(deps.storage, &(position_value - fee_amount))?;
    }

    // If position is being closed, send the remaining amount to the recipient specified in the position closure request.
    if let Some(recipient) = recipient {
        response = response.add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::SendUusdToRecipient {
                proportion,
                recipient,
            },
        ));
    }

    Ok(response)
}

pub fn pair_uusd_with_mirror_asset_to_provide_liquidity_and_stake(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let state = get_position_state(deps, &env, &context)?;
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;

    // Stop if either UST or mAsset balance is zero, or if the Spectrum mAsset-UST vault doesn't exist.
    if state.mirror_asset_balance.is_zero()
        || state.uusd_balance.is_zero()
        || !check_spectrum_mirror_farm_existence(deps, &context, &mirror_asset_cw20_addr)
    {
        return Ok(Response::default());
    }

    // Find amount of uusd and mAsset to pair together and provide liquidity.
    let info = &state.terraswap_pool_info;
    let uusd_ratio = Decimal::from_ratio(state.uusd_balance, info.terraswap_pool_uusd_amount);
    let mirror_asset_ratio = Decimal::from_ratio(
        state.mirror_asset_balance,
        info.terraswap_pool_mirror_asset_amount,
    );
    let ratio = min(uusd_ratio, mirror_asset_ratio);
    let uusd_provide_amount = info.terraswap_pool_uusd_amount * ratio;
    let mirror_asset_provide_amount = info.terraswap_pool_mirror_asset_amount * ratio;

    // Stop if either the calculated UST or mAsset provide amount is zero due to rounding.
    if uusd_provide_amount.is_zero() || mirror_asset_provide_amount.is_zero() {
        return Ok(Response::default());
    }

    // Allow Terraswap mAsset-UST pair contract to transfer mAsset tokens from us.
    let mut response = Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: info.terraswap_pair_addr.clone(),
            amount: mirror_asset_provide_amount,
            expires: None,
        })?,
        funds: vec![],
    }));

    // Provide liquidity to Terraswap mAsset-UST pool.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: info.terraswap_pair_addr.clone(),
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
    let return_lp_token_amount = std::cmp::min(
        uusd_provide_amount.multiply_ratio(
            state.terraswap_pool_info.lp_token_total_supply,
            info.terraswap_pool_uusd_amount,
        ),
        mirror_asset_provide_amount.multiply_ratio(
            state.terraswap_pool_info.lp_token_total_supply,
            info.terraswap_pool_mirror_asset_amount,
        ),
    );
    Ok(response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: info.lp_token_cw20_addr.clone(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: context.spectrum_mirror_farms_addr.to_string(),
            amount: return_lp_token_amount,
            msg: to_binary(&spectrum_protocol::mirror_farm::Cw20HookMsg::bond {
                asset_token: mirror_asset_cw20_addr.to_string(),
                compound_rate: Some(Decimal::one()),
                staker_addr: None,
            })?,
        })?,
        funds: vec![],
    })))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let manager_addr = MANAGER.load(deps.storage)?;
    let context: Context = deps
        .querier
        .query_wasm_smart(&manager_addr, &ManagerQueryMsg::GetContext {})?;
    match msg {
        QueryMsg::GetPositionInfo {} => to_binary(&query_position_info(deps, &env, &context)?),
        QueryMsg::CheckSpectrumMirrorFarmExistence {
            mirror_asset_cw20_addr,
        } => to_binary(&check_spectrum_mirror_farm_existence(
            deps,
            &context,
            &deps.api.addr_validate(&mirror_asset_cw20_addr)?,
        )),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
