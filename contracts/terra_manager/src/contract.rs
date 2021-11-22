use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::*;

use aperture_common::common::{StrategyAction, StrategyType};

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
        },
        ExecuteMsg::InitStrategy {
            strategy_type,
            action_type,
        } => init_strategy(),
        ExecuteMsg::UpdateStrategy {
            strategy_type,
            action_type,
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
    write_investment_registry(deps.storage, strategy_index, &strategy_manager_addr);
    Ok(Response::default())
}

/// Look up the contract address for the associated strategy. Then delegate any
/// necessary information to that contract for execution.
/// Specifically it does the following:
///   * Look up strategy address
///   * Delegate action and tokens received from caller to strategy contract.
pub fn init_strategy() -> StdResult<Response> {
    Ok(Response::default())
}

/// Same as `init_strategy` but for existing positions.
pub fn update_strategy() -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStrategyManagerAddr { strategy_index } => {
            to_binary(&(read_investment_registry(deps.storage, strategy_index)?))
        }
        QueryMsg::GetPositionInfo { position_id } => to_binary(&(read_config(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
