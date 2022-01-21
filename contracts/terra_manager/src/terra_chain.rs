use aperture_common::byte_util::ByteUtils;
use aperture_common::common::{
    get_position_key, Action, Position, PositionId, Strategy, StrategyId, StrategyLocation,
    StrategyMetadata, StrategyPositionManagerExecuteMsg,
};
use aperture_common::token_util::{
    forward_assets_direct, validate_and_accept_incoming_asset_transfer,
};
use cosmwasm_std::{
    entry_point, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128, Uint64, WasmMsg,
};
use cw_storage_plus::{Bound, PrimaryKey, U128Key};
use terraswap::asset::Asset;

use crate::cross_chain::{
    initiate_outgoing_token_transfer, process_cross_chain_instruction,
    register_external_chain_manager,
};
use crate::state::{
    get_strategy_id_key, CrossChainOutgoingFeeConfig, ADMIN, CROSS_CHAIN_OUTGOING_FEE_CONFIG,
    HOLDER_POSITION_ID_PAIR_SET, NEXT_POSITION_ID, NEXT_STRATEGY_ID, POSITION_ID_TO_HOLDER,
    POSITION_TO_STRATEGY_LOCATION_MAP, STRATEGY_ID_TO_METADATA_MAP, WORMHOLE_CORE_BRIDGE_ADDR,
    WORMHOLE_TOKEN_BRIDGE_ADDR,
};
use aperture_common::terra_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, NextPositionIdResponse, PositionInfoResponse,
    PositionsResponse, QueryMsg, TERRA_CHAIN_ID,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    ADMIN.save(deps.storage, &info.sender)?;
    WORMHOLE_TOKEN_BRIDGE_ADDR.save(
        deps.storage,
        &deps.api.addr_validate(&msg.wormhole_token_bridge_addr)?,
    )?;
    WORMHOLE_CORE_BRIDGE_ADDR.save(
        deps.storage,
        &deps.api.addr_validate(&msg.wormhole_core_bridge_addr)?,
    )?;
    CROSS_CHAIN_OUTGOING_FEE_CONFIG.save(
        deps.storage,
        &CrossChainOutgoingFeeConfig {
            rate: msg.cross_chain_outgoing_fee_rate,
            fee_collector_addr: deps
                .api
                .addr_validate(&msg.cross_chain_outgoing_fee_collector_addr)?,
        },
    )?;
    NEXT_STRATEGY_ID.save(deps.storage, &Uint64::zero())?;
    NEXT_POSITION_ID.save(deps.storage, &Uint128::zero())?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::AddStrategy {
            name,
            version,
            manager_addr,
        } => add_strategy(deps, info, name, version, manager_addr),
        ExecuteMsg::RemoveStrategy { strategy_id } => remove_strategy(deps, info, strategy_id),
        ExecuteMsg::RegisterExternalChainManager {
            chain_id,
            aperture_manager_addr,
        } => register_external_chain_manager(deps, info, chain_id, aperture_manager_addr),
        ExecuteMsg::CreatePosition {
            strategy,
            data,
            assets,
        } => create_position(deps, env, info, strategy, data, assets),
        ExecuteMsg::ExecuteStrategy {
            position,
            action,
            assets,
        } => execute_strategy(deps.as_ref(), env, info, position, action, assets),
        ExecuteMsg::ProcessCrossChainInstruction {
            instruction_vaa,
            token_transfer_vaas,
        } => process_cross_chain_instruction(deps, env, instruction_vaa, token_transfer_vaas),
        ExecuteMsg::InitiateOutgoingTokenTransfer { assets, recipient } => {
            initiate_outgoing_token_transfer(deps.as_ref(), env, info, assets, recipient)
        }
    }
}

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
    position: Position,
    action: Action,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let holder = POSITION_ID_TO_HOLDER.load(deps.storage, position.position_id.u128().into())?;
    if holder != info.sender {
        return Err(StdError::generic_err("unauthorized"));
    }
    Ok(
        Response::new().add_messages(create_execute_strategy_messages(
            deps,
            env,
            Some(info),
            position,
            action,
            assets,
        )?),
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStrategyMetadata { strategy_id } => to_binary(
            &STRATEGY_ID_TO_METADATA_MAP.load(deps.storage, get_strategy_id_key(strategy_id))?,
        ),
        QueryMsg::GetNextPositionId {} => to_binary(&NextPositionIdResponse {
            next_position_id: NEXT_POSITION_ID.load(deps.storage)?,
        }),
        QueryMsg::GetTerraPositionInfo { position_id } => to_binary(&PositionInfoResponse {
            holder: POSITION_ID_TO_HOLDER
                .load(deps.storage, position_id.u128().into())?
                .to_string(),
            strategy_location: POSITION_TO_STRATEGY_LOCATION_MAP.load(
                deps.storage,
                get_position_key(&Position {
                    chain_id: TERRA_CHAIN_ID,
                    position_id,
                }),
            )?,
        }),
        QueryMsg::GetTerraPositionsByHolder {
            holder,
            start_after,
            limit,
        } => {
            let mut position_id_vec: Vec<PositionId> = vec![];
            let mut strategy_location_vec: Vec<StrategyLocation> = vec![];
            let min = start_after.map(|position_id| {
                Bound::Exclusive(U128Key::from(position_id.u128()).joined_key())
            });
            const DEFAULT_LIMIT: u32 = 10;
            const MAX_LIMIT: u32 = 30;
            let mut remaining = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
            let positions = HOLDER_POSITION_ID_PAIR_SET
                .prefix(deps.api.addr_validate(&holder)?)
                .range(deps.storage, min, None, cosmwasm_std::Order::Ascending);
            for position in positions {
                if remaining == 0 {
                    break;
                }
                remaining -= 1;
                let (position_id_key, ()) = position?;
                let position_id = Uint128::from(position_id_key.as_slice().get_u128_be(0));
                position_id_vec.push(position_id);
                strategy_location_vec.push(POSITION_TO_STRATEGY_LOCATION_MAP.load(
                    deps.storage,
                    get_position_key(&Position {
                        chain_id: TERRA_CHAIN_ID,
                        position_id,
                    }),
                )?);
            }
            to_binary(&PositionsResponse {
                position_id_vec,
                strategy_location_vec,
            })
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
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
