use std::cmp::Ordering;

use aperture_common::{
    anchor_util::get_anchor_ust_balance_with_uusd_value,
    delta_neutral_position::{
        DetailedPositionInfo, PositionInfoResponse, PositionState, TerraswapPoolInfo,
    },
    delta_neutral_position_manager::{Context, FeeCollectionConfig},
    mirror_util::{
        get_mirror_asset_config_response, get_mirror_asset_oracle_uusd_price_response,
        get_mirror_cdp_response, is_mirror_asset_delisted,
    },
};
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, StdResult, Uint128,
    WasmMsg,
};
use cw_storage_plus::{Item, Map};
use mirror_protocol::collateral_oracle::CollateralPriceResponse;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    dex_util::{
        compute_terraswap_offer_amount, create_terraswap_cw20_uusd_pair_asset_info,
        get_terraswap_mirror_asset_uusd_liquidity_info,
    },
    spectrum_util::unstake_lp_from_spectrum_and_withdraw_liquidity,
    state::{
        CDP_IDX, CDP_PREEMPTIVELY_CLOSED, MANAGER, MIRROR_ASSET_CW20_ADDR, POSITION_CLOSE_INFO,
        POSITION_OPEN_INFO, TARGET_COLLATERAL_RATIO_RANGE,
    },
};

// The minimum allowed width for the target collateral ratio range.
pub const MIN_TARGET_CR_RANGE_WIDTH: &str = "0.4";

pub fn get_uusd_asset_from_amount(amount: Uint128) -> Asset {
    Asset {
        info: AssetInfo::NativeToken {
            denom: "uusd".into(),
        },
        amount,
    }
}

pub fn get_uusd_coin_from_amount(amount: Uint128) -> Coin {
    Coin {
        denom: "uusd".into(),
        amount,
    }
}

pub fn get_uusd_balance(querier: &QuerierWrapper, env: &Env) -> StdResult<Uint128> {
    terraswap::querier::query_balance(querier, env.contract.address.clone(), "uusd".to_string())
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
    let cdp_response = get_mirror_cdp_response(&deps.querier, context, CDP_IDX.load(deps.storage)?);
    let collateral_price_response: CollateralPriceResponse = deps.querier.query_wasm_smart(
        context.mirror_collateral_oracle_addr.clone(),
        &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
            asset: context.anchor_ust_cw20_addr.to_string(),
            timeframe: None,
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
    let mut spectrum_auto_compound_share_amount = Uint128::zero();
    for info in spectrum_info.reward_infos.iter() {
        if info.asset_token == mirror_asset_cw20_addr {
            lp_token_amount = info.bond_amount;
            spectrum_auto_compound_share_amount = info.auto_bond_share;
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
        mirror_asset_short_amount: cdp_response
            .as_ref()
            .map_or(Uint128::zero(), |cdp_response| cdp_response.asset.amount),
        mirror_asset_balance,
        mirror_asset_long_farm,
        mirror_asset_long_amount: mirror_asset_balance.checked_add(mirror_asset_long_farm)?,
        collateral_anchor_ust_amount: cdp_response
            .as_ref()
            .map_or(Uint128::zero(), |cdp_response| {
                cdp_response.collateral.amount
            }),
        collateral_uusd_value: cdp_response
            .as_ref()
            .map_or(Uint128::zero(), |cdp_response| {
                cdp_response.collateral.amount * collateral_price_response.rate
            }),
        mirror_asset_oracle_price: get_mirror_asset_oracle_uusd_price_response(
            &deps.querier,
            context,
            &mirror_asset_cw20_addr,
        )?
        .rate,
        anchor_ust_oracle_price: collateral_price_response.rate,
        terraswap_pool_info: TerraswapPoolInfo {
            lp_token_amount,
            lp_token_cw20_addr,
            lp_token_total_supply,
            terraswap_pair_addr,
            terraswap_pool_mirror_asset_amount,
            terraswap_pool_uusd_amount,
            spectrum_auto_compound_share_amount,
        },
    };
    Ok(state)
}

pub fn increase_mirror_asset_balance_from_long_farm(
    state: &PositionState,
    spectrum_mirror_farms_addr: &Addr,
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
    // Terraswap's withdraw_liquidity implementation: https://github.com/terraswap/terraswap/blob/f4d8a845bc4346db23cb559ce577bc59b41b23cb/contracts/terraswap_pair/src/contract.rs#L312.
    // First, it calculates `share_ratio` as Decimal::from_ratio(redeemed_lp_token_amount, lp_token_total_supply).
    // Then, each of the two tokens in the pair is released in the amount of pool_asset_amount * share_ratio.
    // For example, mAsset released amount is pool_mAsset_amount * share_ratio.
    // Given cosmwasm's implementation of Uint128 and Decimal, this is mathematically equivalent to
    // floor(pool_mAsset_amount * floor(redeemed_lp_token_amount * 1e18 / lp_token_total_supply) / 1e18).
    //
    // Note that the expression `pool_mAsset_amount.multiply_ratio(redeemed_lp_token_amount, lp_token_total_supply)` is equivalent to
    // floor(pool_mAsset_amount * redeemed_lp_token_amount / lp_token_total_supply). Thus, this is not equivalent to the Terraswap implementation described above.
    //
    // Hence, we have to do `pool_mAsset_amount * Decimal::from_ratio(redeemed_lp_token_amount, lp_token_total_supply)` in order to match Terraswap implementation.
    while state.terraswap_pool_info.terraswap_pool_mirror_asset_amount
        * Decimal::from_ratio(
            withdraw_lp_token_amount,
            state.terraswap_pool_info.lp_token_total_supply,
        )
        < withdraw_mirror_asset_amount
    {
        withdraw_lp_token_amount += Uint128::from(1u128);
    }

    unstake_lp_from_spectrum_and_withdraw_liquidity(
        &state.terraswap_pool_info,
        spectrum_mirror_farms_addr,
        mirror_asset_cw20_addr,
        withdraw_lp_token_amount,
    )
}

#[test]
fn test_increase_mirror_asset_balance_from_long_farm() {
    let state = PositionState {
        uusd_balance: Uint128::zero(),
        uusd_long_farm: Uint128::from(151396812u128),
        mirror_asset_short_amount: Uint128::from(10219520u128),
        mirror_asset_balance: Uint128::from(640760u128),
        mirror_asset_long_farm: Uint128::from(8924723u128),
        mirror_asset_long_amount: Uint128::from(640760u128 + 8924723u128),
        collateral_anchor_ust_amount: Uint128::from(294335732u128),
        collateral_uusd_value: Uint128::from(353202878u128),
        mirror_asset_oracle_price: Decimal::from_ratio(155149u128, 10000u128),
        anchor_ust_oracle_price: Decimal::from_ratio(12u128, 10u128),
        terraswap_pool_info: TerraswapPoolInfo {
            lp_token_amount: Uint128::from(35195917u128),
            lp_token_cw20_addr: String::from("lp_token_cw20"),
            lp_token_total_supply: Uint128::from(215043294146u128),
            terraswap_pair_addr: String::from("terraswap_pair"),
            terraswap_pool_mirror_asset_amount: Uint128::from(54529109845u128),
            terraswap_pool_uusd_amount: Uint128::from(924941217839u128),
            spectrum_auto_compound_share_amount: Uint128::from(335195917u128),
        },
    };
    let target_mirror_asset_balance = Uint128::from(1684481u128);

    let withdraw_mirror_asset_amount = target_mirror_asset_balance - state.mirror_asset_balance;
    let estimated_withdraw_lp_token_amount = state
        .terraswap_pool_info
        .lp_token_amount
        .multiply_ratio(withdraw_mirror_asset_amount, state.mirror_asset_long_farm);
    // Assert that due to rounding, the withdrawn mAsset amount plus the original balance is not enough to hit the target amount.
    assert_eq!(
        state.mirror_asset_balance
            + state.terraswap_pool_info.terraswap_pool_mirror_asset_amount
                * Decimal::from_ratio(
                    estimated_withdraw_lp_token_amount,
                    state.terraswap_pool_info.lp_token_total_supply,
                ),
        Uint128::from(1684480u128)
    );
    assert_eq!(
        estimated_withdraw_lp_token_amount,
        Uint128::from(4116062u128)
    );

    // increase_mirror_asset_balance_from_long_farm() is expected to account for the rounding error, and withdraw one more LP token than the estimate.
    let expected_withdraw_lp_token_amount = Uint128::from(4116063u128);
    assert_eq!(
        increase_mirror_asset_balance_from_long_farm(
            &state,
            &Addr::unchecked("spectrum_mirror_farms"),
            &Addr::unchecked("mirror_asset_cw20"),
            target_mirror_asset_balance
        ),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: String::from("spectrum_mirror_farms"),
                funds: vec![],
                msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                    asset_token: String::from("mirror_asset_cw20"),
                    amount: expected_withdraw_lp_token_amount,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: String::from("lp_token_cw20"),
                funds: vec![],
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: String::from("terraswap_pair"),
                    amount: expected_withdraw_lp_token_amount,
                    msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
                })
                .unwrap(),
            })
        ]
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
        cdp_idx: CDP_IDX.may_load(deps.storage)?,
        mirror_asset_cw20_addr: MIRROR_ASSET_CW20_ADDR.load(deps.storage)?,
        detailed_info: None,
    };

    // Position is closed.
    if response.position_close_info.is_some() {
        return Ok(response);
    }

    // CDP is inactive because:
    // (1) this position was opened when oracle price was stale, currently pending DN setup; OR
    // (2) the CDP has been preemptively closed and the funds are currently in Anchor Earn.
    let cdp_preemptively_closed = CDP_PREEMPTIVELY_CLOSED.may_load(deps.storage)? == Some(true);
    if response.cdp_idx.is_none() || cdp_preemptively_closed {
        let (_, anchor_earn_uusd_value) = get_anchor_ust_balance_with_uusd_value(
            deps,
            env,
            &context.anchor_market_addr,
            &context.anchor_ust_cw20_addr,
        )?;
        response.detailed_info = Some(DetailedPositionInfo {
            cdp_preemptively_closed,
            state: None,
            target_collateral_ratio_range: TARGET_COLLATERAL_RATIO_RANGE.load(deps.storage)?,
            collateral_ratio: None,
            unclaimed_short_proceeds_uusd_amount: Uint128::zero(),
            claimable_short_proceeds_uusd_amount: Uint128::zero(),
            claimable_mir_reward_uusd_value: Uint128::zero(),
            claimable_spec_reward_uusd_value: Uint128::zero(),
            uusd_value: anchor_earn_uusd_value,
        });
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
    let collateral_ratio = if state.mirror_asset_short_amount.is_zero() {
        None
    } else {
        Some(Decimal::from_ratio(
            state.collateral_uusd_value,
            state.mirror_asset_short_amount * state.mirror_asset_oracle_price,
        ))
    };

    response.detailed_info = Some(DetailedPositionInfo {
        cdp_preemptively_closed,
        state: Some(state),
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

pub fn get_fee_collection_config_from_manager(deps: Deps) -> StdResult<FeeCollectionConfig> {
    let manager_addr = MANAGER.load(deps.storage)?;
    const FEE_COLLECTION_CONFIG: Item<FeeCollectionConfig> = Item::new("fee_collection_config");
    FEE_COLLECTION_CONFIG.query(&deps.querier, manager_addr)
}

// Determines whether the CDP should be closed due to preemptive setting by the manager or because the mAsset is already delisted from Mirror.
pub fn should_close_cdp(
    deps: Deps,
    mirror_mint_addr: &Addr,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<bool> {
    let manager_addr = MANAGER.load(deps.storage)?;
    const SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS: Map<Addr, bool> = Map::new("spccma");
    return Ok(SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS.query(
        &deps.querier,
        manager_addr,
        mirror_asset_cw20_addr.clone(),
    )? == Some(true)
        || is_mirror_asset_delisted(&get_mirror_asset_config_response(
            &deps.querier,
            mirror_mint_addr,
            mirror_asset_cw20_addr.as_str(),
        )?));
}
