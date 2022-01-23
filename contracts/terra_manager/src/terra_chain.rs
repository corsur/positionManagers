use aperture_common::common::{
    get_position_key, Action, Position, PositionId, Strategy, StrategyId, StrategyLocation,
    StrategyMetadata, StrategyPositionManagerExecuteMsg,
};
use aperture_common::token_util::{
    forward_assets_direct, validate_and_accept_incoming_asset_transfer,
};
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint64, WasmMsg,
};
use cw_storage_plus::U128Key;
use terraswap::asset::Asset;

use crate::state::{
    get_strategy_id_key, ADMIN, HOLDER_POSITION_ID_PAIR_SET, NEXT_POSITION_ID, NEXT_STRATEGY_ID,
    POSITION_ID_TO_HOLDER, POSITION_TO_STRATEGY_LOCATION_MAP, STRATEGY_ID_TO_METADATA_MAP,
};
use aperture_common::terra_manager::TERRA_CHAIN_ID;

pub fn add_strategy(
    deps: DepsMut,
    info: MessageInfo,
    name: String,
    version: String,
    manager_addr: String,
) -> StdResult<Response> {
    if info.sender != ADMIN.load(deps.storage)? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let strategy_id = NEXT_STRATEGY_ID.load(deps.storage)?;
    NEXT_STRATEGY_ID.save(deps.storage, &(strategy_id.checked_add(1u64.into())?))?;
    STRATEGY_ID_TO_METADATA_MAP.save(
        deps.storage,
        get_strategy_id_key(strategy_id),
        &StrategyMetadata {
            name,
            version,
            manager_addr: deps.api.addr_validate(&manager_addr)?,
        },
    )?;
    Ok(Response::default())
}

pub fn remove_strategy(
    deps: DepsMut,
    info: MessageInfo,
    strategy_id: Uint64,
) -> StdResult<Response> {
    if info.sender != ADMIN.load(deps.storage)? {
        return Err(StdError::generic_err("unauthorized"));
    }

    STRATEGY_ID_TO_METADATA_MAP.remove(deps.storage, get_strategy_id_key(strategy_id));
    Ok(Response::default())
}

pub fn create_position(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    strategy: Strategy,
    data: Option<Binary>,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    if strategy.chain_id != TERRA_CHAIN_ID {
        return Err(StdError::generic_err(
            "incorrect entrypoint for cross-chain position creation",
        ));
    }

    // Assign position id.
    let position_id = NEXT_POSITION_ID.load(deps.storage)?;
    NEXT_POSITION_ID.save(deps.storage, &position_id.checked_add(1u128.into())?)?;

    // Save position holder information.
    let position_id_key = U128Key::from(position_id.u128());
    POSITION_ID_TO_HOLDER.save(deps.storage, position_id_key.clone(), &info.sender)?;
    HOLDER_POSITION_ID_PAIR_SET.save(deps.storage, (info.sender.clone(), position_id_key), &())?;

    let position = Position {
        chain_id: TERRA_CHAIN_ID,
        position_id,
    };
    Ok(
        Response::new().add_messages(save_new_position_info_and_open_it(
            deps,
            env,
            Some(info),
            position,
            strategy.strategy_id,
            data,
            assets,
        )?),
    )
}

pub fn save_new_position_info_and_open_it(
    deps: DepsMut,
    env: Env,
    info: Option<MessageInfo>,
    position: Position,
    strategy_id: StrategyId,
    data: Option<Binary>,
    assets: Vec<Asset>,
) -> StdResult<Vec<CosmosMsg>> {
    // Save position -> strategy mapping.
    let position_key = get_position_key(&position);
    POSITION_TO_STRATEGY_LOCATION_MAP.save(
        deps.storage,
        position_key,
        &StrategyLocation::TerraChain(strategy_id),
    )?;

    create_execute_strategy_messages(
        deps.as_ref(),
        env,
        info,
        position,
        Action::OpenPosition { data },
        assets,
    )
}

pub fn execute_strategy(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    position_id: PositionId,
    action: Action,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let holder = POSITION_ID_TO_HOLDER.load(deps.storage, U128Key::from(position_id.u128()))?;
    if holder != info.sender {
        return Err(StdError::generic_err("unauthorized"));
    }
    Ok(
        Response::new().add_messages(create_execute_strategy_messages(
            deps,
            env,
            Some(info),
            Position {
                chain_id: TERRA_CHAIN_ID,
                position_id,
            },
            action,
            assets,
        )?),
    )
}

fn get_terra_strategy_id(strategy_location: StrategyLocation) -> StdResult<StrategyId> {
    match strategy_location {
        StrategyLocation::ExternalChain(_) => Err(StdError::generic_err(
            "Cross-chain action not yet supported",
        )),
        StrategyLocation::TerraChain(strategy_id) => Ok(strategy_id),
    }
}

pub fn create_execute_strategy_messages(
    deps: Deps,
    env: Env,
    info: Option<MessageInfo>,
    position: Position,
    action: Action,
    assets: Vec<Asset>,
) -> StdResult<Vec<CosmosMsg>> {
    let strategy_location =
        POSITION_TO_STRATEGY_LOCATION_MAP.load(deps.storage, get_position_key(&position))?;
    let strategy_id = get_terra_strategy_id(strategy_location)?;
    let strategy_manager_addr = STRATEGY_ID_TO_METADATA_MAP
        .load(deps.storage, get_strategy_id_key(strategy_id))?
        .manager_addr;

    // Validate & accept incoming asset transfer.
    let mut messages: Vec<CosmosMsg> = if let Some(info) = info {
        validate_and_accept_incoming_asset_transfer(env, info, &assets)?
    } else {
        vec![]
    };

    // Forward assets to the strategy manager.
    let (funds, cw20_transfer_messages) = forward_assets_direct(&assets, &strategy_manager_addr)?;
    messages.extend(cw20_transfer_messages);

    // Ask strategy manager to perform the requested action.
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: strategy_manager_addr.to_string(),
        msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
            position,
            action,
            assets,
        })?,
        funds,
    }));
    Ok(messages)
}
