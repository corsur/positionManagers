use cosmwasm_std::{Api, Decimal, Extern, Querier, StdResult, Storage, Uint128};

const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000u128);

pub fn decimal_division(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(DECIMAL_FRACTIONAL * a, b * DECIMAL_FRACTIONAL)
}

pub fn decimal_multiplication(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(a * DECIMAL_FRACTIONAL * b, DECIMAL_FRACTIONAL)
}

pub fn get_tax_cap_in_uusd<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>) -> StdResult<Uint128> {
    match terra_cosmwasm::TerraQuerier::new(&deps.querier).query_tax_cap(String::from("uusd")) {
        Ok(response) => Ok(response.cap),
        Err(err) => Err(err),
    }
}
