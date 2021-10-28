use cosmwasm_std::{
    entry_point, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdResult, Uint128, WasmMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, WormholeTokenBridgeExecuteMsg};
use crate::state::*;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        amadeus_addr: deps.api.addr_validate(&msg.amadeus_addr)?,
        wormhole_token_bridge_addr: deps.api.addr_validate(&msg.wormhole_token_bridge_addr)?,
    };
    write_config(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, _info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::ClaimTokensFromWormholeAndDeltaNeutralInvest {
            vaa,
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        } => claims_tokens_from_wormhole_and_delta_neutral_invest(
            deps,
            env,
            vaa,
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        ),
        ExecuteMsg::DeltaNeutralInvest {
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        } => delta_neutral_invest(
            deps,
            env,
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        ),
    }
}

pub fn delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let uusd_balance = terraswap::querier::query_balance(
        &deps.querier,
        env.contract.address,
        String::from("uusd"),
    )?;
    let uusd_asset = terraswap::asset::Asset {
        amount: uusd_balance,
        info: terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    };
    Ok(Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.amadeus_addr.to_string(),
            msg: to_binary(&ExecuteMsg::DeltaNeutralInvest {
                collateral_ratio_in_percentage,
                mirror_asset_cw20_addr,
            })?,
            funds: vec![uusd_asset.deduct_tax(&deps.querier)?],
        })))
}

pub fn claims_tokens_from_wormhole_and_delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    vaa: Binary,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    Ok(Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.wormhole_token_bridge_addr.to_string(),
            msg: to_binary(&WormholeTokenBridgeExecuteMsg::SubmitVaa {
                data: vaa,
            })?,
            funds: vec![],
        })).add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::DeltaNeutralInvest {
                collateral_ratio_in_percentage,
                mirror_asset_cw20_addr,
            })?,
            funds: vec![],
        })))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
