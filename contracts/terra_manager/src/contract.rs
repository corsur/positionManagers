use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Storage, WasmMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{OWNER, STRATEGIES};
use crate::util::get_strategy_key;

use aperture_common::common::{StrategyAction, StrategyType, TokenInfo};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    OWNER.save(deps.storage, &info.sender)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    let is_authorized = info.sender == OWNER.load(deps.storage)?;
    match msg {
        // Owner only: updates investment strategy bucket is priviledged.
        ExecuteMsg::RegisterInvestment {
            strategy_type,
            strategy_manager_addr,
        } => {
            if !is_authorized {
                return Err(StdError::GenericErr {
                    msg: "Unauthorized".to_string(),
                });
            }
            register_investment(deps, strategy_type, strategy_manager_addr)
        }
        // Public methods.
        ExecuteMsg::InitStrategy {
            strategy_type,
            action_type,
            token_type,
        } => init_strategy(deps.storage, strategy_type, action_type, token_type),
        // Public methods.
        ExecuteMsg::UpdateStrategy {
            strategy_type: _,
            action_type: _,
            token_type: _,
            position_id: _,
        } => update_strategy(),
    }
}

/// Owner only. Register strategy_type and the corresponding strategy manager's
/// address into storage.
pub fn register_investment(
    deps: DepsMut,
    strategy_type: StrategyType,
    strategy_manager_addr: String,
) -> StdResult<Response> {
    let validated_addr: Addr = deps.api.addr_validate(&strategy_manager_addr).unwrap();
    STRATEGIES.save(
        deps.storage,
        get_strategy_key(&strategy_type),
        &validated_addr,
    )?;
    Ok(Response::default())
}

/// Look up the contract address for the associated strategy. Then delegate any
/// necessary information to that contract for execution.
/// Specifically it does the following:
///   * Look up strategy address.
///   * Delegate action and tokens received from caller to strategy contract.
pub fn init_strategy(
    storage: &dyn Storage,
    strategy_type: StrategyType,
    action: StrategyAction,
    token: TokenInfo,
) -> StdResult<Response> {
    // Step 1: look up strategy address.
    let strategy_addr: Addr = STRATEGIES.load(storage, get_strategy_key(&strategy_type))?;

    // Step 2: Transfer fund/token to strategy contract.
    let is_native = token.native;
    let native_coin = Coin {
        denom: String::from("uusd"),
        amount: token.amount,
    };
    let funds: Vec<Coin> = if is_native { vec![native_coin] } else { vec![] };
    // TODO: add logic for non-native CW20 tokens.

    // Step 3: Issue CW-721 token as a receipt to the user.

    // Step 4: delegate action and funds to strategy contract.
    match strategy_type {
        StrategyType::DeltaNeutral(params) => Ok(Response::new().add_message(CosmosMsg::Wasm(
            WasmMsg::Execute {
                contract_addr: strategy_addr.to_string(),
                msg: to_binary(
                    &aperture_common::delta_neutral_position_manager::ExecuteMsg::Do {
                        action,
                        token,
                        params,
                    },
                )?,
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
            to_binary(&STRATEGIES.load(deps.storage, get_strategy_key(&strategy_type))?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
