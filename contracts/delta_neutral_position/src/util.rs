use std::{cmp::Ordering, convert::TryFrom, str::FromStr};

use aperture_common::{
    delta_neutral_position::{
        DetailedPositionInfo, PositionInfoResponse, PositionState, TargetCollateralRatioRange,
        TerraswapPoolInfo,
    },
    delta_neutral_position_manager::Context,
};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Decimal256, Deps, Env, QuerierWrapper, StdError,
    StdResult, Uint128, Uint256, WasmMsg,
};
use mirror_protocol::collateral_oracle::CollateralPriceResponse;
use terraswap::asset::{Asset, AssetInfo, PairInfo};

use crate::{
    dex_util::create_terraswap_cw20_uusd_pair_asset_info,
    state::{
        INITIAL_DEPOSIT_UUSD_AMOUNT, POSITION_CLOSE_BLOCK_INFO, POSITION_INFO,
        POSITION_OPEN_BLOCK_INFO, TARGET_COLLATERAL_RATIO_RANGE,
    },
};

const DECIMAL_FRACTIONAL: Uint128 = Uint128::new(1_000_000_000u128);

pub fn decimal_multiplication(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(a * DECIMAL_FRACTIONAL * b, DECIMAL_FRACTIONAL)
}

pub fn decimal_division(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(DECIMAL_FRACTIONAL * a, b * DECIMAL_FRACTIONAL)
}

pub fn decimal_inverse(decimal: Decimal) -> Decimal {
    Decimal::from_ratio(DECIMAL_FRACTIONAL, decimal * DECIMAL_FRACTIONAL)
}

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
    let position_info = POSITION_INFO.load(deps.storage)?;
    deps.querier.query_wasm_smart(
        &context.mirror_lock_addr,
        &mirror_protocol::lock::QueryMsg::PositionLockInfo {
            position_idx: position_info.cdp_idx,
        },
    )
}

pub fn get_terraswap_uusd_mirror_asset_pool_balance_info(
    deps: Deps,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<(PairInfo, Uint128, Uint128)> {
    let terraswap_pair_asset_info =
        create_terraswap_cw20_uusd_pair_asset_info(mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr.clone(),
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
    Ok((
        terraswap_pair_info,
        pool_mirror_asset_balance,
        pool_uusd_balance,
    ))
}

pub fn find_collateral_uusd_amount(
    deps: Deps,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
    target_collateral_ratio_range: &TargetCollateralRatioRange,
    uusd_amount: Uint128,
) -> StdResult<(Uint128, Uint128)> {
    let terraswap_pair_asset_info =
        create_terraswap_cw20_uusd_pair_asset_info(mirror_asset_cw20_addr);
    let mirror_asset_info = &terraswap_pair_asset_info[0];
    let uusd_asset_info = &terraswap_pair_asset_info[1];

    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr.clone(),
        &terraswap_pair_asset_info,
    )?;
    let terraswap_pair_contract_addr =
        deps.api.addr_validate(&terraswap_pair_info.contract_addr)?;
    let pool_mirror_asset_balance = mirror_asset_info.query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr.clone(),
    )?;
    let pool_uusd_balance =
        uusd_asset_info.query_pool(&deps.querier, deps.api, terraswap_pair_contract_addr)?;

    // Obtain aUST collateral and mAsset information.
    let aust_collateral_info_response: mirror_protocol::collateral_oracle::CollateralInfoResponse =
        deps.querier.query_wasm_smart(
            context.mirror_collateral_oracle_addr.clone(),
            &mirror_protocol::collateral_oracle::QueryMsg::CollateralAssetInfo {
                asset: context.anchor_ust_cw20_addr.to_string(),
            },
        )?;
    let mirror_asset_config_response: mirror_protocol::mint::AssetConfigResponse =
        deps.querier.query_wasm_smart(
            context.mirror_mint_addr.clone(),
            &mirror_protocol::mint::QueryMsg::AssetConfig {
                asset_token: mirror_asset_cw20_addr.to_string(),
            },
        )?;

    // Abort if mAsset is delisted.
    if mirror_asset_config_response.end_price.is_some() {
        return Err(StdError::GenericErr {
            msg: "mAsset is delisted".to_string(),
        });
    }

    // Obtain mAsset oracle price.
    let mirror_asset_oracle_price = get_mirror_asset_oracle_uusd_price(
        &deps.querier,
        context,
        mirror_asset_cw20_addr.as_str(),
    )?;

    // Check that target_min_collateral_ratio meets the safety margin requirement, i.e. exceeds the minimum threshold by at least the configured safety margin.
    let min_collateral_ratio = decimal_multiplication(
        mirror_asset_config_response.min_collateral_ratio,
        aust_collateral_info_response.multiplier,
    );
    if target_collateral_ratio_range.min
        < min_collateral_ratio + context.collateral_ratio_safety_margin
    {
        return Err(StdError::GenericErr {
            msg: "target_min_collateral_ratio too small".to_string(),
        });
    }

    // Our goal is to find the maximum mAsset amount to mint such that `uusd_amount` is able to cover:
    // (1) uusd_amount_for_collateral = target_collateral_ratio * (mAsset amount * mAsset oracle price).
    // (2) uusd_amount_for_long_position which is able to get us the same mAsset amount from Terraswap after the short position is opened.
    // We perform a binary search for the amount of mAsset to find the maximum that satisfies these constraints.
    // TODO: Consider whether Uint128 is enough for numerator and denominator.
    let mut a = Uint128::zero();
    let mut b = uusd_amount * decimal_inverse(mirror_asset_oracle_price);
    let collateral_to_mirror_asset_amount_ratio = decimal_multiplication(
        mirror_asset_oracle_price,
        target_collateral_ratio_range.midpoint(),
    );
    while b - a > 1u128.into() {
        // Check whether it is possible to mint m amount of mAsset.
        let m = (a + b) >> 1;

        let uusd_for_long_position = pool_uusd_balance
            * Decimal::from_ratio(
                m * (pool_mirror_asset_balance * Uint128::from(1000u128)
                    + m * Uint128::from(3u128)),
                (pool_mirror_asset_balance + m)
                    * (pool_mirror_asset_balance * Uint128::from(997u128)
                        - m * Uint128::from(3u128)),
            );
        let uusd_for_collateral = m * collateral_to_mirror_asset_amount_ratio;
        if uusd_for_collateral + uusd_for_long_position <= uusd_amount {
            a = m;
        } else {
            b = m;
        }
    }
    Ok((a, a * collateral_to_mirror_asset_amount_ratio))
}

pub fn get_position_state(deps: Deps, env: &Env, context: &Context) -> StdResult<PositionState> {
    let position_info = POSITION_INFO.load(deps.storage)?;
    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Position {
                position_idx: position_info.cdp_idx,
            },
        )?;
    let collateral_price_response: CollateralPriceResponse = deps.querier.query_wasm_smart(
        context.mirror_collateral_oracle_addr.clone(),
        &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
            asset: context.anchor_ust_cw20_addr.to_string(),
            block_height: None,
        },
    )?;

    let spectrum_info: spectrum_protocol::mirror_farm::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            context.spectrum_mirror_farms_addr.to_string(),
            &spectrum_protocol::mirror_farm::QueryMsg::reward_info {
                staker_addr: env.contract.address.to_string(),
                asset_token: Some(position_info.mirror_asset_cw20_addr.to_string()),
            },
        )?;
    let mut terraswap_pool_info: Option<TerraswapPoolInfo> = None;
    let mut uusd_long_farm = Uint128::zero();
    let mut mirror_asset_long_farm = Uint128::zero();
    for info in spectrum_info.reward_infos.iter() {
        if info.asset_token == position_info.mirror_asset_cw20_addr {
            let lp_token_amount = info.bond_amount;

            let asset_infos =
                create_terraswap_cw20_uusd_pair_asset_info(&position_info.mirror_asset_cw20_addr);
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
            mirror_asset_long_farm = lp_token_amount
                .multiply_ratio(terraswap_pool_mirror_asset_amount, lp_token_total_supply);
            let terraswap_pool_uusd_amount =
                asset_infos[1].query_pool(&deps.querier, deps.api, validated_pair_addr)?;
            uusd_long_farm =
                lp_token_amount.multiply_ratio(terraswap_pool_uusd_amount, lp_token_total_supply);

            terraswap_pool_info = Some(TerraswapPoolInfo {
                lp_token_amount,
                lp_token_cw20_addr,
                lp_token_total_supply,
                terraswap_pair_addr,
                terraswap_pool_mirror_asset_amount,
                terraswap_pool_uusd_amount,
            });
        }
    }

    let mirror_asset_balance = terraswap::querier::query_token_balance(
        &deps.querier,
        position_info.mirror_asset_cw20_addr.clone(),
        env.contract.address.clone(),
    )?;
    let state = PositionState {
        uusd_balance: terraswap::querier::query_balance(
            &deps.querier,
            env.contract.address.clone(),
            "uusd".into(),
        )?,
        uusd_long_farm,
        mirror_asset_short_amount: position_response.asset.amount,
        mirror_asset_balance,
        mirror_asset_long_farm,
        mirror_asset_long_amount: mirror_asset_balance.checked_add(mirror_asset_long_farm)?,
        collateral_anchor_ust_amount: position_response.collateral.amount,
        collateral_uusd_value: position_response.collateral.amount * collateral_price_response.rate,
        mirror_asset_cw20_addr: position_info.mirror_asset_cw20_addr.clone(),
        mirror_asset_oracle_price: get_mirror_asset_oracle_uusd_price(
            &deps.querier,
            context,
            position_info.mirror_asset_cw20_addr.as_str(),
        )?,
        anchor_ust_oracle_price: collateral_price_response.rate,
        terraswap_pool_info,
    };
    Ok(state)
}

pub fn unstake_lp_and_withdraw_liquidity(
    state: &PositionState,
    context: &Context,
    withdraw_lp_token_amount: Uint128,
) -> Vec<CosmosMsg> {
    vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_mirror_farms_addr.to_string(),
            funds: vec![],
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                asset_token: state.mirror_asset_cw20_addr.to_string(),
                amount: withdraw_lp_token_amount,
            })
            .unwrap(),
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state
                .terraswap_pool_info
                .as_ref()
                .unwrap()
                .lp_token_cw20_addr
                .to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: state
                    .terraswap_pool_info
                    .as_ref()
                    .unwrap()
                    .terraswap_pair_addr
                    .to_string(),
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
    target_mirror_asset_balance: Uint128,
) -> Vec<CosmosMsg> {
    if target_mirror_asset_balance <= state.mirror_asset_balance {
        return vec![];
    }
    let withdraw_mirror_asset_amount = target_mirror_asset_balance - state.mirror_asset_balance;
    let withdraw_lp_token_amount = state
        .terraswap_pool_info
        .as_ref()
        .unwrap()
        .lp_token_amount
        .multiply_ratio(withdraw_mirror_asset_amount, state.mirror_asset_long_farm);
    unstake_lp_and_withdraw_liquidity(state, context, withdraw_lp_token_amount)
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

pub fn simulate_terraswap_swap(
    offer_pool_amount: Uint128,
    ask_pool_amount: Uint128,
    offer_amount: Uint128,
) -> (Uint128, Uint128, Uint128) {
    let new_offer_pool_amount = offer_pool_amount + offer_amount;
    let cp = offer_pool_amount.full_mul(ask_pool_amount);
    let one = Uint256::from(1u128);
    let commission_rate = Decimal256::from_str("0.003").unwrap();
    let return_amount = (Decimal256::from_ratio(ask_pool_amount, one)
        - Decimal256::from_ratio(cp, new_offer_pool_amount))
        * one;
    let return_amount = Uint128::try_from(return_amount - return_amount * commission_rate).unwrap();
    (
        new_offer_pool_amount,
        ask_pool_amount - return_amount,
        return_amount,
    )
}

pub fn compute_terraswap_uusd_offer_amount(
    deps: Deps,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
    ask_mirror_asset_amount: Uint128,
) -> StdResult<(PairInfo, Uint128)> {
    let (terraswap_pair_info, pool_mirror_asset_balance, pool_uusd_balance) =
        get_terraswap_uusd_mirror_asset_pool_balance_info(deps, context, mirror_asset_cw20_addr)?;
    let pool_mirror_asset_balance: Uint256 = pool_mirror_asset_balance.into();
    let pool_uusd_balance: Uint256 = pool_uusd_balance.into();
    let commission_rate = Decimal256::from_str("0.003").unwrap();
    let cp = pool_mirror_asset_balance * pool_uusd_balance;

    let mut a = 0u128;
    let mut b = u128::MAX >> 1;
    let one = Uint256::from(1u128);
    while b - a > 1u128 {
        let offer_uusd_amount = (a + b) >> 1;
        let mut return_mirror_asset_amount =
            (Decimal256::from_ratio(pool_mirror_asset_balance, one)
                - Decimal256::from_ratio(cp, pool_uusd_balance + Uint256::from(offer_uusd_amount)))
                * one;
        return_mirror_asset_amount =
            return_mirror_asset_amount - return_mirror_asset_amount * commission_rate;
        if return_mirror_asset_amount <= ask_mirror_asset_amount.into() {
            a = offer_uusd_amount;
        } else {
            b = offer_uusd_amount;
        }
    }
    Ok((terraswap_pair_info, a.into()))
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
    terraswap_factory_addr: Addr,
    cw20_token_addr: &Addr,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        querier,
        terraswap_factory_addr,
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
        initial_deposit_uusd_amount: INITIAL_DEPOSIT_UUSD_AMOUNT.load(deps.storage)?,
        position_open_block_info: POSITION_OPEN_BLOCK_INFO.load(deps.storage)?,
        position_close_block_info: POSITION_CLOSE_BLOCK_INFO.may_load(deps.storage)?,
        detailed_info: None,
    };
    if response.position_close_block_info.is_some() {
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
        context.terraswap_factory_addr.clone(),
        &context.spectrum_cw20_addr,
        find_unclaimed_spec_amount(deps, env, context)?,
    )?;
    value = value.checked_add(spec_uusd_value)?;
    // Unclaimed MIR reward.
    let mir_uusd_value = find_cw20_token_uusd_value(
        &deps.querier,
        context.terraswap_factory_addr.clone(),
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
                context.terraswap_factory_addr.clone(),
                &state.mirror_asset_cw20_addr,
                net_long_amount,
            )?)?;
        }
        Ordering::Less => {
            let net_short_amount = state.mirror_asset_short_amount - state.mirror_asset_long_amount;
            value = value.checked_sub(
                compute_terraswap_uusd_offer_amount(
                    deps,
                    context,
                    &state.mirror_asset_cw20_addr,
                    net_short_amount,
                )?
                .1,
            )?;
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
