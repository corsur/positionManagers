use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::*;

/// Instantiate the contract with basic configuration persisted in the storage.
/// Note that owner is set at this time.
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

/// Dispatch message to its corresponding function call.
/// Operations in this block is privileged. Only owner can modify the internal
/// state of the contract. Other caller wishes to get information should be
/// query against this contract. 
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let is_authorized = info.sender == config.owner;
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "Unauthorized".to_string(),
        });
    }
    // Updates to the internal is privileged.
    match msg {
        ExecuteMsg::RegisterInvestment {
            strategy_index,
            strategy_manager_addr,
        } => register_investment(deps, strategy_index, strategy_manager_addr),
    }
}

/// Owner only. Register strategy_index and the corresponding strategy manager's
/// address into storage.
pub fn register_investment(
    deps: DepsMut,
    strategy_index: u64,
    strategy_manager_addr: Addr,
) -> StdResult<Response> {
    write_investment_registry(deps.storage, strategy_index, &strategy_manager_addr);
    Ok(Response::default())
}

/// Query method to return information related to strategy contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStrategyManagerAddr { strategy_index } => {
            to_binary(&(read_investment_registry(deps.storage, strategy_index)?))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
