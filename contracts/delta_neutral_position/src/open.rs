use aperture_common::{
    delta_neutral_position::TargetCollateralRatioRange, delta_neutral_position_manager::Context,
};
use cosmwasm_bignumber::Uint256;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, DepsMut, Env, Response, StdError, StdResult,
    Uint128, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    dex_util::{
        compute_terraswap_offer_amount, get_terraswap_mirror_asset_uusd_liquidity_info,
        simulate_terraswap_swap,
    },
    math::{decimal_division, reverse_decimal},
    util::{get_mirror_asset_oracle_uusd_price, get_uusd_asset_from_amount},
};

pub fn delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    context: Context,
    uusd_amount: Uint128,
    target_collateral_ratio_range: &TargetCollateralRatioRange,
    mirror_asset_cw20_addr: &Addr,
    cdp_idx: Option<Uint128>,
) -> StdResult<Response> {
    let (pair_info, pool_mirror_asset_balance, pool_uusd_balance) =
        get_terraswap_mirror_asset_uusd_liquidity_info(
            deps.as_ref(),
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
        &context,
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
            &context,
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
