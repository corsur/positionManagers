use std::cmp::Ordering;

use aperture_common::{
    delta_neutral_position::{
        DetailedPositionInfo, PositionInfoResponse, PositionState, TerraswapPoolInfo,
    },
    delta_neutral_position_manager::Context,
};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, StdResult, Uint128, WasmMsg,
};
use mirror_protocol::collateral_oracle::CollateralPriceResponse;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    dex_util::{
        compute_terraswap_offer_amount, create_terraswap_cw20_uusd_pair_asset_info,
        get_terraswap_mirror_asset_uusd_liquidity_info,
    },
    state::{
        CDP_IDX, MIRROR_ASSET_CW20_ADDR, POSITION_CLOSE_INFO, POSITION_OPEN_INFO,
        TARGET_COLLATERAL_RATIO_RANGE,
    },
};

pub fn get_uusd_asset_from_amount(amount: Uint128) -> Asset {
    Asset {
        info: AssetInfo::NativeToken {
            denom: "uusd".into(),
        },
        amount,
    }
}

pub fn get_uusd_balance(querier: &QuerierWrapper, env: &Env) -> StdResult<Uint128> {
    terraswap::querier::query_balance(querier, env.contract.address.clone(), "uusd".to_string())
}

pub fn get_mirror_asset_oracle_uusd_price(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &str,
) -> StdResult<Decimal> {
    let mirror_asset_price_response: mirror_protocol::oracle::PriceResponse = querier
        .query_wasm_smart(
            context.mirror_oracle_addr.clone(),
            &mirror_protocol::oracle::QueryMsg::Price {
                base_asset: mirror_asset_cw20_addr.to_string(),
                quote_asset: "uusd".to_string(),
            },
        )?;
    Ok(mirror_asset_price_response.rate)
}

pub fn get_cdp_uusd_lock_info_result(
    deps: Deps,
    context: &Context,
) -> StdResult<mirror_protocol::lock::PositionLockInfoResponse> {
    deps.querier.query_wasm_smart(
        &context.mirror_lock_addr,
        &mirror_protocol::lock::QueryMsg::PositionLockInfo {
            position_idx: CDP_IDX.load(deps.storage)?,
        },
    )
}

pub fn get_position_state(deps: Deps, env: &Env, context: &Context) -> StdResult<PositionState> {
    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Position {
                position_idx: CDP_IDX.load(deps.storage)?,
            },
        )?;
    let collateral_price_response: CollateralPriceResponse = deps.querier.query_wasm_smart(
        context.mirror_collateral_oracle_addr.clone(),
        &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
            asset: context.anchor_ust_cw20_addr.to_string(),
            block_height: None,
        },
    )?;
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
    let spectrum_info: spectrum_protocol::mirror_farm::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            context.spectrum_mirror_farms_addr.to_string(),
            &spectrum_protocol::mirror_farm::QueryMsg::reward_info {
                staker_addr: env.contract.address.to_string(),
                asset_token: Some(mirror_asset_cw20_addr.to_string()),
            },
        )?;

    let mut lp_token_amount = Uint128::zero();
    for info in spectrum_info.reward_infos.iter() {
        if info.asset_token == mirror_asset_cw20_addr {
            lp_token_amount = info.bond_amount;
        }
    }
    let asset_infos = create_terraswap_cw20_uusd_pair_asset_info(&mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr.clone(),
        &asset_infos,
    )?;
    let terraswap_pair_addr = terraswap_pair_info.contract_addr;
    let validated_pair_addr = deps.api.addr_validate(&terraswap_pair_addr)?;
    let lp_token_cw20_addr = terraswap_pair_info.liquidity_token;
    let lp_token_total_supply = terraswap::querier::query_supply(
        &deps.querier,
        deps.api.addr_validate(&lp_token_cw20_addr)?,
    )?;

    let terraswap_pool_mirror_asset_amount =
        asset_infos[0].query_pool(&deps.querier, deps.api, validated_pair_addr.clone())?;
    let terraswap_pool_uusd_amount =
        asset_infos[1].query_pool(&deps.querier, deps.api, validated_pair_addr)?;

    let mirror_asset_balance = terraswap::querier::query_token_balance(
        &deps.querier,
        mirror_asset_cw20_addr.clone(),
        env.contract.address.clone(),
    )?;
    let mirror_asset_long_farm =
        lp_token_amount.multiply_ratio(terraswap_pool_mirror_asset_amount, lp_token_total_supply);
    let state = PositionState {
        uusd_balance: terraswap::querier::query_balance(
            &deps.querier,
            env.contract.address.clone(),
            "uusd".into(),
        )?,
        uusd_long_farm: lp_token_amount
            .multiply_ratio(terraswap_pool_uusd_amount, lp_token_total_supply),
        mirror_asset_short_amount: position_response.asset.amount,
        mirror_asset_balance,
        mirror_asset_long_farm,
        mirror_asset_long_amount: mirror_asset_balance.checked_add(mirror_asset_long_farm)?,
        collateral_anchor_ust_amount: position_response.collateral.amount,
        collateral_uusd_value: position_response.collateral.amount * collateral_price_response.rate,
        mirror_asset_oracle_price: get_mirror_asset_oracle_uusd_price(
            &deps.querier,
            context,
            mirror_asset_cw20_addr.as_str(),
        )?,
        anchor_ust_oracle_price: collateral_price_response.rate,
        terraswap_pool_info: TerraswapPoolInfo {
            lp_token_amount,
            lp_token_cw20_addr,
            lp_token_total_supply,
            terraswap_pair_addr,
            terraswap_pool_mirror_asset_amount,
            terraswap_pool_uusd_amount,
        },
    };
    Ok(state)
}

pub fn unstake_lp_and_withdraw_liquidity(
    state: &PositionState,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
    withdraw_lp_token_amount: Uint128,
) -> Vec<CosmosMsg> {
    vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_mirror_farms_addr.to_string(),
            funds: vec![],
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                asset_token: mirror_asset_cw20_addr.to_string(),
                amount: withdraw_lp_token_amount,
            })
            .unwrap(),
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.terraswap_pool_info.lp_token_cw20_addr.to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: state.terraswap_pool_info.terraswap_pair_addr.to_string(),
                amount: withdraw_lp_token_amount,
                msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
            })
            .unwrap(),
        }),
    ]
}

pub fn increase_mirror_asset_balance_from_long_farm(
    state: &PositionState,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
    target_mirror_asset_balance: Uint128,
) -> Vec<CosmosMsg> {
    if target_mirror_asset_balance <= state.mirror_asset_balance {
        return vec![];
    }
    let withdraw_mirror_asset_amount = target_mirror_asset_balance - state.mirror_asset_balance;
    let mut withdraw_lp_token_amount = state
        .terraswap_pool_info
        .lp_token_amount
        .multiply_ratio(withdraw_mirror_asset_amount, state.mirror_asset_long_farm);

    // Due to rounding, `withdraw_lp_token_amount` may not redeem for `withdraw_mirror_asset_amount` amount of mAsset, so we adjust it up if necessary.
    while state
        .terraswap_pool_info
        .terraswap_pool_mirror_asset_amount
        .multiply_ratio(
            withdraw_lp_token_amount,
            state.terraswap_pool_info.lp_token_amount,
        )
        < withdraw_mirror_asset_amount
    {
        withdraw_lp_token_amount += Uint128::from(1u128);
    }

    unstake_lp_and_withdraw_liquidity(
        state,
        context,
        mirror_asset_cw20_addr,
        withdraw_lp_token_amount,
    )
}

pub fn increase_uusd_balance_from_aust_collateral(
    context: &Context,
    cdp_idx: Uint128,
    anchor_ust_amount: Uint128,
) -> Vec<CosmosMsg> {
    vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.mirror_mint_addr.to_string(),
            funds: vec![],
            msg: to_binary(&mirror_protocol::mint::ExecuteMsg::Withdraw {
                position_idx: cdp_idx,
                collateral: Some(Asset {
                    info: AssetInfo::Token {
                        contract_addr: context.anchor_ust_cw20_addr.to_string(),
                    },
                    amount: anchor_ust_amount,
                }),
            })
            .unwrap(),
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_ust_cw20_addr.to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.anchor_market_addr.to_string(),
                amount: anchor_ust_amount,
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {}).unwrap(),
            })
            .unwrap(),
        }),
    ]
}

pub fn find_unclaimed_spec_amount(deps: Deps, env: &Env, context: &Context) -> StdResult<Uint128> {
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
    Ok(spec_reward)
}

pub fn find_unclaimed_mir_amount(deps: Deps, env: &Env, context: &Context) -> StdResult<Uint128> {
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
    Ok(mir_reward)
}

pub fn find_cw20_token_uusd_value(
    querier: &QuerierWrapper,
    terraswap_factory_addr: &Addr,
    cw20_token_addr: &Addr,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        querier,
        terraswap_factory_addr.clone(),
        &create_terraswap_cw20_uusd_pair_asset_info(cw20_token_addr),
    )?;
    Ok(terraswap::querier::simulate(
        querier,
        Addr::unchecked(terraswap_pair_info.contract_addr),
        &Asset {
            info: AssetInfo::Token {
                contract_addr: cw20_token_addr.to_string(),
            },
            amount,
        },
    )?
    .return_amount)
}

pub fn query_position_info(
    deps: Deps,
    env: &Env,
    context: &Context,
) -> StdResult<PositionInfoResponse> {
    let mut response = PositionInfoResponse {
        position_open_info: POSITION_OPEN_INFO.load(deps.storage)?,
        position_close_info: POSITION_CLOSE_INFO.may_load(deps.storage)?,
        cdp_idx: CDP_IDX.load(deps.storage)?,
        mirror_asset_cw20_addr: MIRROR_ASSET_CW20_ADDR.load(deps.storage)?,
        detailed_info: None,
    };
    if response.position_close_info.is_some() {
        return Ok(response);
    }

    let state = get_position_state(deps, env, context)?;
    let position_lock_info_result = get_cdp_uusd_lock_info_result(deps, context);
    let mut unclaimed_short_proceeds_uusd_amount = Uint128::zero();
    let mut claimable_short_proceeds_uusd_amount = Uint128::zero();
    if let Ok(response) = position_lock_info_result {
        unclaimed_short_proceeds_uusd_amount = response.locked_amount;
        if response.unlock_time <= env.block.time.seconds() {
            claimable_short_proceeds_uusd_amount = response.locked_amount;
        }
    };

    // Native UST.
    let mut value = state
        .uusd_balance
        .checked_add(state.uusd_long_farm)?
        .checked_add(unclaimed_short_proceeds_uusd_amount)?;
    // Collateral aUST.
    value = value.checked_add(state.collateral_uusd_value)?;
    // Unclaimed SPEC reward.
    let spec_uusd_value = find_cw20_token_uusd_value(
        &deps.querier,
        &context.terraswap_factory_addr,
        &context.spectrum_cw20_addr,
        find_unclaimed_spec_amount(deps, env, context)?,
    )?;
    value = value.checked_add(spec_uusd_value)?;
    // Unclaimed MIR reward.
    let mir_uusd_value = find_cw20_token_uusd_value(
        &deps.querier,
        &context.terraswap_factory_addr,
        &context.mirror_cw20_addr,
        find_unclaimed_mir_amount(deps, env, context)?,
    )?;
    value = value.checked_add(mir_uusd_value)?;
    // mAsset value.
    match state
        .mirror_asset_long_amount
        .cmp(&state.mirror_asset_short_amount)
    {
        Ordering::Greater => {
            let net_long_amount = state.mirror_asset_long_amount - state.mirror_asset_short_amount;
            value = value.checked_add(find_cw20_token_uusd_value(
                &deps.querier,
                &context.terraswap_factory_addr,
                &MIRROR_ASSET_CW20_ADDR.load(deps.storage)?,
                net_long_amount,
            )?)?;
        }
        Ordering::Less => {
            let net_short_amount = state.mirror_asset_short_amount - state.mirror_asset_long_amount;
            let (_, pool_mirror_asset_amount, pool_uusd_amount) =
                get_terraswap_mirror_asset_uusd_liquidity_info(
                    deps,
                    &context.terraswap_factory_addr,
                    &MIRROR_ASSET_CW20_ADDR.load(deps.storage)?,
                )?;
            value = value.checked_sub(compute_terraswap_offer_amount(
                pool_mirror_asset_amount,
                pool_uusd_amount,
                net_short_amount,
            )?)?;
        }
        Ordering::Equal => {}
    }
    // Current collateral ratio.
    let collateral_ratio = Decimal::from_ratio(
        state.collateral_uusd_value,
        state.mirror_asset_short_amount * state.mirror_asset_oracle_price,
    );

    response.detailed_info = Some(DetailedPositionInfo {
        state,
        target_collateral_ratio_range: TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?,
        collateral_ratio,
        unclaimed_short_proceeds_uusd_amount,
        claimable_short_proceeds_uusd_amount,
        claimable_mir_reward_uusd_value: mir_uusd_value,
        claimable_spec_reward_uusd_value: spec_uusd_value,
        uusd_value: value,
    });
    Ok(response)
}
