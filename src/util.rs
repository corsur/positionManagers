use cosmwasm_std::{Api, Decimal, QuerierWrapper, StdResult, Uint128};
use terraswap::asset::AssetInfo;

const DECIMAL_FRACTIONAL: Uint128 = Uint128::new(1_000_000_000u128);

pub fn inverse_decimal(decimal: Decimal) -> Decimal {
    Decimal::from_ratio(DECIMAL_FRACTIONAL, decimal * DECIMAL_FRACTIONAL)
}

pub fn get_tax_cap_in_uusd(querier: &QuerierWrapper) -> StdResult<Uint128> {
    match terra_cosmwasm::TerraQuerier::new(querier).query_tax_cap(String::from("uusd")) {
        Ok(response) => Ok(response.cap),
        Err(err) => Err(err),
    }
}

pub fn get_terraswap_pair_asset_info(cw20_token_addr: &str) -> [AssetInfo; 2] {
    [
        terraswap::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.to_string(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

pub fn get_uusd_amount_to_swap_for_long_position(
    querier: &QuerierWrapper,
    api: &dyn Api,
    terraswap_pair_addr: &str,
    mirror_asset_info: &AssetInfo,
    uusd_asset_info: &AssetInfo,
    minted_mirror_asset_amount: Uint128,
) -> StdResult<Uint128> {
    let one_minus_commission_rate = Decimal::from_ratio(997u128, 1000u128);
    let reverse_one_minus_commission_rate = Decimal::from_ratio(1000u128, 997u128);

    // Initial pool balances.
    let mut balance_mirror_asset =
        mirror_asset_info.query_pool(querier, api, api.addr_validate(terraswap_pair_addr)?)?;
    let mut balance_uusd =
        uusd_asset_info.query_pool(querier, api, api.addr_validate(terraswap_pair_addr)?)?;

    // Simulate short sale (mirror_asset -> uusd).
    let cp = Uint128::new(balance_mirror_asset.u128() * balance_uusd.u128());
    let uusd_amount_before_fee = balance_uusd
        .checked_sub(cp.multiply_ratio(1u128, balance_mirror_asset + minted_mirror_asset_amount))?;
    let uusd_amount_after_fee = uusd_amount_before_fee * one_minus_commission_rate;
    balance_mirror_asset += minted_mirror_asset_amount;
    balance_uusd = balance_uusd.checked_sub(uusd_amount_after_fee)?;

    // Simulate long buy (uusd -> mirror_asset).
    let cp = Uint128::new(balance_mirror_asset.u128() * balance_uusd.u128());
    let uusd_amount_to_swap_without_tax: Uint128 = cp
        .multiply_ratio(
            1u128,
            balance_mirror_asset
                .checked_sub(minted_mirror_asset_amount * reverse_one_minus_commission_rate)?,
        )
        .checked_sub(balance_uusd)?;
    Ok(uusd_amount_to_swap_without_tax)
}
