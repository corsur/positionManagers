use cosmwasm_std::{Api, Decimal, Extern, HumanAddr, Querier, StdResult, Storage, Uint128};
use terraswap::asset::AssetInfo;

const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000u128);

pub fn inverse_decimal(decimal: Decimal) -> Decimal {
    Decimal::from_ratio(DECIMAL_FRACTIONAL, decimal * DECIMAL_FRACTIONAL)
}

pub fn get_tax_cap_in_uusd<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>) -> StdResult<Uint128> {
    match terra_cosmwasm::TerraQuerier::new(&deps.querier).query_tax_cap(String::from("uusd")) {
        Ok(response) => Ok(response.cap),
        Err(err) => Err(err),
    }
}

pub fn get_terraswap_pair_asset_info(mirror_asset_cw20_addr: &HumanAddr) -> [AssetInfo; 2] {
    return [
        terraswap::asset::AssetInfo::Token {
            contract_addr: mirror_asset_cw20_addr.clone(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ];
}

pub fn get_uusd_amount_to_swap_for_long_position<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    terraswap_pair_addr: &HumanAddr,
    mirror_asset_info: &AssetInfo,
    uusd_asset_info: &AssetInfo,
    minted_mirror_asset_amount: Uint128) -> StdResult<Uint128> {
    let one_minus_commission_rate = Decimal::from_ratio(997u128, 1000u128);
    let reverse_one_minus_commission_rate = Decimal::from_ratio(1000u128, 997u128);

    // Initial pool balances.
    let mut balance_mirror_asset = mirror_asset_info.query_pool(deps, terraswap_pair_addr)?;
    let mut balance_uusd = uusd_asset_info.query_pool(deps, terraswap_pair_addr)?;

    // Simulate short sale (mirror_asset -> uusd).
    let cp = Uint128(balance_mirror_asset.u128() * balance_uusd.u128());
    let uusd_amount_before_fee = (balance_uusd - cp.multiply_ratio(1u128, balance_mirror_asset + minted_mirror_asset_amount))?;
    let uusd_amount_after_fee = uusd_amount_before_fee * one_minus_commission_rate;
    balance_mirror_asset += minted_mirror_asset_amount;
    balance_uusd = (balance_uusd - uusd_amount_after_fee).unwrap();

    // Simulate long buy (uusd -> mirror_asset).
    let cp = Uint128(balance_mirror_asset.u128() * balance_uusd.u128());
    let uusd_amount_to_swap_without_tax: Uint128 = (cp.multiply_ratio(
            1u128, (balance_mirror_asset - minted_mirror_asset_amount * reverse_one_minus_commission_rate)?) - balance_uusd)?;
    Ok(uusd_amount_to_swap_without_tax)
}
