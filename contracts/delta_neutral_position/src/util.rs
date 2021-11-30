use cosmwasm_std::{to_binary, Addr, CosmosMsg, QuerierWrapper, StdResult, Uint128, WasmMsg};
use terraswap::asset::AssetInfo;

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
