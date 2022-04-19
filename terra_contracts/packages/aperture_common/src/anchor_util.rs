use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{Addr, Deps, Env, StdResult, Uint128};

pub fn get_anchor_ust_exchange_rate(
    deps: Deps,
    env: &Env,
    anchor_market_addr: &Addr,
) -> StdResult<Decimal256> {
    let anchor_market_epoch_state: moneymarket::market::EpochStateResponse =
        deps.querier.query_wasm_smart(
            anchor_market_addr.to_string(),
            &moneymarket::market::QueryMsg::EpochState {
                block_height: Some(env.block.height),
                distributed_interest: None,
            },
        )?;
    Ok(anchor_market_epoch_state.exchange_rate)
}

// Given `anchor_ust_amount` and the exchange rate for aUST/UST, returns the amount of uusd that would be returned if the specified aUST is redeemed.
// The conversion to Uint256 and then back to Uint128 is necessary to match the Anchor money market contract logic.
// Reference: https://github.com/Anchor-Protocol/money-market-contracts/blob/b99f561411f6ac8ca9622172a3e86047e8f4334f/contracts/market/src/deposit.rs#L84
pub fn get_anchor_ust_redemption_uusd_value(
    anchor_ust_amount: Uint128,
    anchor_ust_exchange_rate: Decimal256,
) -> Uint128 {
    (Uint256::from(anchor_ust_amount) * anchor_ust_exchange_rate).into()
}

// Calculates the uusd amount that would be returned if the entire aUST balance held by this contract is redeemed.
// Returns (aUST_balance, uusd_value).
pub fn get_anchor_ust_balance_with_uusd_value(
    deps: Deps,
    env: &Env,
    anchor_market_addr: &Addr,
    anchor_ust_cw20_addr: &Addr,
) -> StdResult<(Uint128, Uint128)> {
    let anchor_market_epoch_state: moneymarket::market::EpochStateResponse =
        deps.querier.query_wasm_smart(
            anchor_market_addr.to_string(),
            &moneymarket::market::QueryMsg::EpochState {
                block_height: Some(env.block.height),
                distributed_interest: None,
            },
        )?;
    let anchor_ust_balance = terraswap::querier::query_token_balance(
        &deps.querier,
        anchor_ust_cw20_addr.clone(),
        env.contract.address.clone(),
    )?;
    Ok((
        anchor_ust_balance,
        get_anchor_ust_redemption_uusd_value(
            anchor_ust_balance,
            anchor_market_epoch_state.exchange_rate,
        ),
    ))
}
