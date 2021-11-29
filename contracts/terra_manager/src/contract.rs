use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QueryRequest, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128,
    WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::*;

use aperture_common::common::{StrategyAction, StrategyType, TokenInfo};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config { owner: info.sender };
    write_config(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let is_authorized = info.sender == config.owner;

    match msg {
        // Owner only: updates investment strategy bucket is priviledged.
        ExecuteMsg::RegisterInvestment {
            strategy_index,
            strategy_manager_addr,
        } => {
            if !is_authorized {
                return Err(StdError::GenericErr {
                    msg: "Unauthorized".to_string(),
                });
            }
            register_investment(deps, strategy_index, strategy_manager_addr)
        }
        ExecuteMsg::InitStrategy {
            strategy_type,
            action_type,
            token_type,
        } => init_strategy(deps.storage, strategy_type, action_type, token_type),
        ExecuteMsg::UpdateStrategy {
            strategy_type,
            action_type,
            token_type,
            position_id,
        } => update_strategy(),
    }
}

/// Owner only. Register strategy_index and the corresponding strategy manager's
/// address into storage.
pub fn register_investment(
    deps: DepsMut,
    strategy_index: StrategyType,
    strategy_manager_addr: Addr,
) -> StdResult<Response> {
    write_investment_registry(deps.storage, strategy_index, &strategy_manager_addr)?;
    Ok(Response::default())
}

/// Look up the contract address for the associated strategy. Then delegate any
/// necessary information to that contract for execution.
/// Specifically it does the following:
///   * Look up strategy address.
///   * Delegate action and tokens received from caller to strategy contract.
pub fn init_strategy(
    storage: &dyn Storage,
    strategy: StrategyType,
    action: StrategyAction,
    token: TokenInfo,
) -> StdResult<Response> {
    // Step 1: look up strategy address.
    let strategy_addr = read_investment_registry(storage, &strategy)?;

    // Step 2: Transfer fund/token to strategy contract.
    let is_native = token.native;
    let native_coin = Coin {
        denom: String::from("uusd"),
        amount: token.amount,
    };
    let funds = if is_native { vec![native_coin] } else { vec![] };
    // TODO: add logic for non-native CW20 tokens.

    // Step 3: Issue CW-721 token as a receipt to the user.

    // Step 4: delegate action and funds to strategy contract.
    match strategy {
        StrategyType::DeltaNeutral(params) => Ok(Response::new().add_message(CosmosMsg::Wasm(
            WasmMsg::Execute {
                contract_addr: strategy_addr.to_string(),
                msg: to_binary(&aperture_common::delta_neutral_manager::ExecuteMsg::Do {
                    action,
                    token,
                    params,
                })?,
                funds,
            },
        ))),
    }
}

/// Same as `init_strategy` but for existing positions.
pub fn update_strategy() -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStrategyManagerAddr { strategy_type } => {
            to_binary(&(read_investment_registry(deps.storage, &strategy_type)?))
        }
        QueryMsg::GetPositionInfo { position_id } => to_binary(&(read_config(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
