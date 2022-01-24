use std::cmp::Ordering;

use aperture_common::{
    delta_neutral_position::{
        DetailedPositionInfo, PositionInfoResponse, PositionState, TargetCollateralRatioRange,
        TerraswapPoolInfo,
    },
    delta_neutral_position_manager::Context,
};
use cosmwasm_bignumber::Uint256;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, Response, StdError,
    StdResult, Uint128, WasmMsg,
};
use mirror_protocol::collateral_oracle::CollateralPriceResponse;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    dex_util::{
        compute_terraswap_offer_amount, create_terraswap_cw20_uusd_pair_asset_info,
        get_terraswap_mirror_asset_uusd_liquidity_info, simulate_terraswap_swap,
    },
    math::{decimal_division, reverse_decimal},
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

pub fn find_collateral_uusd_amount(
    deps: Deps,
    env: &Env,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
    target_collateral_ratio_range: &TargetCollateralRatioRange,
    uusd_amount: Uint128,
    cdp_idx: Option<Uint128>,
) -> StdResult<Response> {
    let (pair_info, pool_mirror_asset_balance, pool_uusd_balance) =
        get_terraswap_mirror_asset_uusd_liquidity_info(
            deps,
            &context.terraswap_factory_addr,
            mirror_asset_cw20_addr,
        )?;

    // Obtain mAsset information.
    let mirror_asset_config_response: mirror_protocol::mint::AssetConfigResponse =
        deps.querier.query_wasm_smart(
            context.mirror_mint_addr.clone(),
            &mirror_protocol::mint::QueryMsg::AssetConfig {
                asset_token: mirror_asset_cw20_addr.to_string(),
            },
        )?;

    // Abort if mAsset is delisted.
    if mirror_asset_config_response.end_price.is_some() {
        return Err(StdError::generic_err("mAsset is delisted"));
    }

    // Obtain mAsset oracle price.
    let mirror_asset_oracle_price = get_mirror_asset_oracle_uusd_price(
        &deps.querier,
        context,
        mirror_asset_cw20_addr.as_str(),
    )?;

    // Check that target_min_collateral_ratio meets the safety margin requirement, i.e. exceeds the minimum threshold by at least the configured safety margin.
    if target_collateral_ratio_range.min
        < mirror_asset_config_response.min_collateral_ratio + context.collateral_ratio_safety_margin
    {
        return Err(StdError::generic_err(
            "target_min_collateral_ratio too small",
        ));
    }

    // Query Anchor Market epoch state for aUST exchange rate.
    let anchor_market_epoch_state: moneymarket::market::EpochStateResponse =
        deps.querier.query_wasm_smart(
            context.anchor_market_addr.to_string(),
            &moneymarket::market::QueryMsg::EpochState {
                block_height: Some(env.block.height),
                distributed_interest: None,
            },
        )?;
    let anchor_ust_exchange_rate = Decimal::from(anchor_market_epoch_state.exchange_rate);

    // Our goal is to find the maximum amount of uusd that can be posted as collateral (in the form of aUST) such that there is enough uusd remaining that can be swapped for the minted amount of mAsset.
    // We use binary search to achieve this goal.
    let mut a = Uint128::zero();
    let mut b = uusd_amount;
    let collateral_ratio = target_collateral_ratio_range.midpoint();
    let one = Uint128::from(1u128);
    while b > a + one {
        // We post `uusd_collateral_amount` amount of uusd as collateral, and simulate to see what happens.
        let uusd_collateral_amount = (a + b) >> 1;

        // First, we deposit `uusd_collateral_amount` amount of uusd into Anchor Market, and get back `collateral_anchor_ust_amount` amount of aUST.
        let collateral_anchor_ust_amount = Uint128::from(
            Uint256::from(uusd_collateral_amount) / anchor_market_epoch_state.exchange_rate,
        );

        // Second, we open a short position via Mirror Mint.
        // With `collateral_anchor_ust_amount` amount of aUST collateral and `collateral_ratio`, Mirror will mint `mirror_asset_mint_amount` amount of mAsset.
        let mirror_asset_mint_amount = collateral_anchor_ust_amount
            * decimal_division(anchor_ust_exchange_rate, mirror_asset_oracle_price)
            * reverse_decimal(collateral_ratio);

        // Third, Mirror will swap `mirror_asset_mint_amount` amount of mAsset for uusd via Terraswap.
        // The Terraswap mAsset-UST pool state will become the following after this swap.
        let (pool_mirror_asset_balance_after_short_swap, pool_uusd_balance_after_short_swap, _) =
            simulate_terraswap_swap(
                pool_mirror_asset_balance,
                pool_uusd_balance,
                mirror_asset_mint_amount,
            );

        // Finally, we want to swap the least amount of uusd for the same `mirror_asset_mint_amount`.
        let uusd_long_swap_amount = compute_terraswap_offer_amount(
            pool_mirror_asset_balance_after_short_swap,
            pool_uusd_balance_after_short_swap,
            mirror_asset_mint_amount,
        );

        // Determine feasibility by checking whether the sum of `uusd_collateral_amount` and `uusd_long_swap_amount` stays within our budget of `uusd_amount`.
        let feasible = uusd_long_swap_amount.map_or_else(
            |_| false,
            |uusd_long_swap_amount| uusd_collateral_amount + uusd_long_swap_amount <= uusd_amount,
        );
        if feasible {
            a = uusd_collateral_amount;
        } else {
            b = uusd_collateral_amount;
        }
    }

    // Simulate the process one final time using the final `uusd_collateral_amount` value.
    let uusd_collateral_amount = a;
    let collateral_anchor_ust_amount = Uint128::from(
        Uint256::from(uusd_collateral_amount) / anchor_market_epoch_state.exchange_rate,
    );
    let mirror_asset_mint_amount = collateral_anchor_ust_amount
        * decimal_division(anchor_ust_exchange_rate, mirror_asset_oracle_price)
        * reverse_decimal(collateral_ratio);
    let (pool_mirror_asset_balance_after_short_swap, pool_uusd_balance_after_short_swap, _) =
        simulate_terraswap_swap(
            pool_mirror_asset_balance,
            pool_uusd_balance,
            mirror_asset_mint_amount,
        );
    let uusd_long_swap_amount = compute_terraswap_offer_amount(
        pool_mirror_asset_balance_after_short_swap,
        pool_uusd_balance_after_short_swap,
        mirror_asset_mint_amount,
    )?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_collateral_amount,
            }],
        }))
        .add_messages(open_or_increase_cdp(
            context,
            collateral_ratio,
            collateral_anchor_ust_amount,
            mirror_asset_cw20_addr.to_string(),
            mirror_asset_mint_amount,
            cdp_idx,
        )?)
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_info.contract_addr,
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: get_uusd_asset_from_amount(uusd_long_swap_amount),
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_long_swap_amount,
            }],
        }))
        .add_attributes(vec![
            ("collateral_anchor_ust_amount", collateral_anchor_ust_amount),
            ("mirror_asset_mint_amount", mirror_asset_mint_amount),
        ]))
}

fn open_or_increase_cdp(
    context: &Context,
    collateral_ratio: Decimal,
    collateral_anchor_ust_amount: Uint128,
    mirror_asset_cw20_addr: String,
    mirror_asset_mint_amount: Uint128,
    cdp_idx: Option<Uint128>,
) -> StdResult<Vec<CosmosMsg>> {
    match cdp_idx {
        None => Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: collateral_anchor_ust_amount,
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
        })]),
        Some(position_idx) => Ok(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.anchor_ust_cw20_addr.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: context.mirror_mint_addr.to_string(),
                    amount: collateral_anchor_ust_amount,
                    msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Deposit { position_idx })?,
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
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
            }),
        ]),
    }
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
    let mut terraswap_pool_info: Option<TerraswapPoolInfo> = None;
    let mut uusd_long_farm = Uint128::zero();
    let mut mirror_asset_long_farm = Uint128::zero();
    for info in spectrum_info.reward_infos.iter() {
        if info.asset_token == mirror_asset_cw20_addr {
            let lp_token_amount = info.bond_amount;

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
        mirror_asset_cw20_addr.clone(),
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
        mirror_asset_oracle_price: get_mirror_asset_oracle_uusd_price(
            &deps.querier,
            context,
            mirror_asset_cw20_addr.as_str(),
        )?,
        anchor_ust_oracle_price: collateral_price_response.rate,
        terraswap_pool_info,
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
    mirror_asset_cw20_addr: &Addr,
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
