use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, StdError, StdResult, Uint128,
    WasmMsg,
};
use mirror_protocol::collateral_oracle::CollateralPriceResponse;
use terraswap::asset::AssetInfo;

use crate::state::{TargetCollateralRatioRange, POSITION_INFO};

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

/// Returns an array comprising two AssetInfo elements, representing a Terraswap token pair where the first token is a cw20 with contract address
/// `cw20_token_addr` and the second token is the native "uusd" token. The returned array is useful for querying Terraswap for pair info.
/// # Arguments
///
/// * `cw20_token_addr` - Contract address of the specified cw20 token
pub fn create_terraswap_cw20_uusd_pair_asset_info(cw20_token_addr: &str) -> [AssetInfo; 2] {
    [
        terraswap::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.to_string(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

/// Returns a Wasm execute message that swaps the cw20 token at address `cw20_token_addr` in the amount of `amount` for uusd via Terraswap.
///
/// The contract address of the Terraswap cw20-uusd pair is first looked up from the factory. An error is returned if this query fails.
/// If the pair contract lookup is successful, then a message that swaps the specified amount of cw20 tokens for uusd is returned.
///
/// # Arguments
///
/// * `querier` - Reference to a querier which is used to query Terraswap factory
/// * `terraswap_factory_addr` - Address of the Terraswap factory contract
/// * `cw20_token_addr` - Contract address of the cw20 token to be swapped
/// * `amount` - Amount of the cw20 token to be swapped
pub fn swap_cw20_token_for_uusd(
    querier: &QuerierWrapper,
    terraswap_factory_addr: Addr,
    cw20_token_addr: &str,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        querier,
        terraswap_factory_addr,
        &create_terraswap_cw20_uusd_pair_asset_info(cw20_token_addr),
    )?;
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_token_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: terraswap_pair_info.contract_addr,
            amount,
            msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
        })?,
        funds: vec![],
    }))
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

pub fn find_collateral_uusd_amount(
    deps: Deps,
    context: &Context,
    mirror_asset_cw20_addr: &str,
    target_collateral_ratio_range: &TargetCollateralRatioRange,
    mut uusd_amount: Uint128,
) -> StdResult<Uint128> {
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

    // The amount of uusd set aside for tax payment.
    let buffer_amount = (terraswap::asset::Asset {
        amount: uusd_amount,
        info: uusd_asset_info.clone(),
    })
    .compute_tax(&deps.querier)?
    .multiply_ratio(2u128, 1u128);
    uusd_amount = uusd_amount.checked_sub(buffer_amount)?;

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
    let mirror_asset_oracle_price =
        get_mirror_asset_oracle_uusd_price(&deps.querier, context, mirror_asset_cw20_addr)?;

    // Check that target_min_collateral_ratio meets the safety margin requirement, i.e. exceeds the minimum threshold by at least the configured safety margin.
    let min_collateral_ratio = decimal_multiplication(
        mirror_asset_config_response.min_collateral_ratio,
        aust_collateral_info_response.multiplier,
    );
    if min_collateral_ratio + context.collateral_ratio_safety_margin
        < target_collateral_ratio_range.min
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
        let m = (a + b).multiply_ratio(1u128, 2u128);

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
    Ok(a * collateral_to_mirror_asset_amount_ratio)
}

pub struct PositionState {
    // This contract's uusd balance.
    pub uusd_balance: Uint128,
    // Amount of uusd redeemable from staked LP tokens.
    pub uusd_long_farm: Uint128,
    // Amount of shorted mAsset.
    pub mirror_asset_short_amount: Uint128,
    // This contract's mAsset balance.
    pub mirror_asset_balance: Uint128,
    // Amount of mAsset redeemable from staked LP tokens.
    pub mirror_asset_long_farm: Uint128,
    // Amount of aUST collateral.
    pub collateral_anchor_ust_amount: Uint128,
    // Value of aUST colleteral in uusd.
    pub collateral_uusd_value: Uint128,
    // Address of the mAsset cw20 contract.
    pub mirror_asset_cw20_addr: Addr,
    // Oracle price of the mAsset.
    pub mirror_asset_oracle_price: Decimal,
    // Oracle price of aUST.
    pub anchor_ust_oracle_price: Decimal,
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

    let uusd_long_farm = Uint128::zero();
    let mirror_asset_long_farm = Uint128::zero();
    let state = PositionState {
        uusd_balance: terraswap::querier::query_balance(
            &deps.querier,
            env.contract.address.clone(),
            "uusd".into(),
        )?,
        uusd_long_farm,
        mirror_asset_short_amount: position_response.asset.amount,
        mirror_asset_balance: terraswap::querier::query_token_balance(
            &deps.querier,
            position_info.mirror_asset_cw20_addr.clone(),
            env.contract.address.clone(),
        )?,
        mirror_asset_long_farm,
        collateral_anchor_ust_amount: position_response.collateral.amount,
        collateral_uusd_value: position_response.collateral.amount * collateral_price_response.rate,
        mirror_asset_cw20_addr: position_info.mirror_asset_cw20_addr.clone(),
        mirror_asset_oracle_price: get_mirror_asset_oracle_uusd_price(
            &deps.querier,
            context,
            position_info.mirror_asset_cw20_addr.as_str(),
        )?,
        anchor_ust_oracle_price: collateral_price_response.rate,
    };
    Ok(state)
}

pub fn increase_mirror_asset_balance_from_long_farm(
    state: &PositionState,
    target_mirror_asset_balance: Uint128,
) -> Vec<CosmosMsg> {
    if target_mirror_asset_balance <= state.mirror_asset_balance {
        return vec![];
    }
    // TODO
    vec![]
}
