use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::*;

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

/// Dispatch enum message to its corresponding functions.
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
        ExecuteMsg::Action(aperture_common::common::StrategyAction::OpenPosition {}) => {
            open_position()
        }
        ExecuteMsg::Action(aperture_common::common::StrategyAction::IncreasePosition {}) => {
            increase_position()
        }
        ExecuteMsg::Action(aperture_common::common::StrategyAction::DecreasePosition {}) => {
            decrease_position()
        }
        ExecuteMsg::Action(aperture_common::common::StrategyAction::ClosePosition {}) => {
            close_position()
        }
    }
}

pub fn open_position() -> StdResult<Response> {
    Ok(Response::default())
}

pub fn increase_position() -> StdResult<Response> {
    Ok(Response::default())
}

pub fn decrease_position() -> StdResult<Response> {
    Ok(Response::default())
}

pub fn close_position() -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position_id } => to_binary(&(read_config(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
