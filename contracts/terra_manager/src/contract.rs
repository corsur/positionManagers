use aperture_common::byte_util::ByteUtils;
use aperture_common::common::{get_position_key, Position, PositionId, StrategyLocation};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    Uint64,
};
use cw_storage_plus::{Bound, PrimaryKey, U128Key};

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
use crate::terra_chain::{add_strategy, create_position, execute_strategy, remove_strategy};
use aperture_common::terra_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, NextPositionIdResponse, PositionInfoResponse,
    PositionsResponse, QueryMsg, TERRA_CHAIN_ID,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    ADMIN.save(deps.storage, &deps.api.addr_validate(&msg.admin_addr)?)?;
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
            position_id,
            action,
            assets,
        } => execute_strategy(deps.as_ref(), env, info, position_id, action, assets),
        ExecuteMsg::ProcessCrossChainInstruction {
            instruction_vaa,
            token_transfer_vaas,
        } => process_cross_chain_instruction(deps, env, instruction_vaa, token_transfer_vaas),
        ExecuteMsg::InitiateOutgoingTokenTransfer { assets, recipient } => {
            initiate_outgoing_token_transfer(deps.as_ref(), env, info, assets, recipient)
        }
    }
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
