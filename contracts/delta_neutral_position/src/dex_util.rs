use cosmwasm_std::{to_binary, Addr, CosmosMsg, QuerierWrapper, StdResult, Uint128, WasmMsg};

/// Returns an array comprising two AssetInfo elements, representing a Terraswap token pair where the first token is a cw20 with contract address
/// `cw20_token_addr` and the second token is the native "uusd" token. The returned array is useful for querying Terraswap for pair info.
///
/// # Arguments
/// * `cw20_token_addr` - Contract address of the specified cw20 token
pub fn create_terraswap_cw20_uusd_pair_asset_info(
    cw20_token_addr: &Addr,
) -> [terraswap::asset::AssetInfo; 2] {
    [
        terraswap::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.to_string(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

/// Returns an array comprising two AssetInfo elements, representing an Astroport token pair where the first token is a cw20 with contract address
/// `cw20_token_addr` and the second token is the native "uusd" token. The returned array is useful for querying Astroport for pair info.
///
/// # Arguments
/// * `cw20_token_addr` - Contract address of the specified cw20 token
fn create_astroport_cw20_uusd_pair_asset_info(
    cw20_token_addr: &Addr,
) -> [astroport::asset::AssetInfo; 2] {
    [
        astroport::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.clone(),
        },
        astroport::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

/// Returns a Wasm execute message that swaps the cw20 token at address `cw20_token_addr` in the amount of `amount` for uusd via Terraswap or Astroport,
/// whichever returning more uusd.
///
/// # Arguments
///
/// * `querier` - Reference to a querier which is used to query Terraswap factory
/// * `terraswap_factory_addr` - Address of the Terraswap factory contract
/// * `astroport_factory_addr` - Address of the Astroport factory contract
/// * `cw20_token_addr` - Contract address of the cw20 token to be swapped
/// * `amount` - Amount of the cw20 token to be swapped
pub fn swap_cw20_token_for_uusd(
    querier: &QuerierWrapper,
    terraswap_factory_addr: &Addr,
    astroport_factory_addr: &Addr,
    cw20_token_addr: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        querier,
        terraswap_factory_addr.clone(),
        &create_terraswap_cw20_uusd_pair_asset_info(cw20_token_addr),
    );
    let terraswap_uusd_return_amount = if let Ok(ref pair_info) = terraswap_pair_info {
        terraswap::querier::simulate(
            querier,
            Addr::unchecked(pair_info.contract_addr.clone()),
            &terraswap::asset::Asset {
                amount,
                info: terraswap::asset::AssetInfo::Token {
                    contract_addr: cw20_token_addr.to_string(),
                },
            },
        )
        .unwrap()
        .return_amount
    } else {
        Uint128::zero()
    };

    let astroport_pair_info = astroport::querier::query_pair_info(
        querier,
        astroport_factory_addr.clone(),
        &create_astroport_cw20_uusd_pair_asset_info(cw20_token_addr),
    );
    let astroport_uusd_return_amount = if let Ok(ref pair_info) = astroport_pair_info {
        astroport::querier::simulate(
            querier,
            pair_info.contract_addr.clone(),
            &astroport::asset::Asset {
                amount,
                info: astroport::asset::AssetInfo::Token {
                    contract_addr: cw20_token_addr.clone(),
                },
            },
        )
        .unwrap()
        .return_amount
    } else {
        Uint128::zero()
    };

    let cw20_execute_msg = if terraswap_uusd_return_amount >= astroport_uusd_return_amount {
        cw20::Cw20ExecuteMsg::Send {
            contract: terraswap_pair_info?.contract_addr,
            amount,
            msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
        }
    } else {
        cw20::Cw20ExecuteMsg::Send {
            contract: astroport_pair_info?.contract_addr.to_string(),
            amount,
            msg: to_binary(&astroport::pair::Cw20HookMsg::Swap {
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
        }
    };
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_token_addr.to_string(),
        msg: to_binary(&cw20_execute_msg)?,
        funds: vec![],
    }))
}
