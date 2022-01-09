use aperture_common::bytes_util::ByteUtils;
use aperture_common::common::{
    get_position_key, Action, Position, PositionId, Strategy, StrategyId, StrategyLocation,
    StrategyMetadata, StrategyPositionManagerExecuteMsg,
};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, Uint64, WasmMsg,
};
use cw_storage_plus::{Bound, PrimaryKey, U128Key};
use terraswap::asset::{Asset, AssetInfo};

use crate::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, NextPositionIdResponse, PositionInfoResponse,
    PositionsResponse, QueryMsg, TERRA_CHAIN_ID,
};
use crate::state::{
    get_strategy_id_key, ADMIN, HOLDER_POSITION_ID_PAIR_SET, NEXT_POSITION_ID, NEXT_STRATEGY_ID,
    POSITION_ID_TO_HOLDER, POSITION_TO_STRATEGY_LOCATION_MAP, STRATEGY_ID_TO_METADATA_MAP,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    ADMIN.save(deps.storage, &info.sender)?;
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
            "cross-chain strategy not yet supported",
        ));
    }

    // Assign position id.
    let position_id = NEXT_POSITION_ID.load(deps.storage)?;
    NEXT_POSITION_ID.save(deps.storage, &position_id.checked_add(1u128.into())?)?;

    // Save position -> strategy mapping.
    let position = Position {
        chain_id: TERRA_CHAIN_ID,
        position_id,
    };
    let position_key = get_position_key(&position);
    POSITION_TO_STRATEGY_LOCATION_MAP.save(
        deps.storage,
        position_key,
        &StrategyLocation::TerraChain(strategy.strategy_id),
    )?;

    // Save position holder information.
    let position_id_key = U128Key::from(position_id.u128());
    POSITION_ID_TO_HOLDER.save(deps.storage, position_id_key.clone(), &info.sender)?;
    HOLDER_POSITION_ID_PAIR_SET.save(deps.storage, (info.sender.clone(), position_id_key), &())?;

    // Emit messages that execute the strategy and issues a cw-721 token to the user at the end.
    Ok(
        Response::new().add_messages(create_execute_strategy_messages(
            deps.as_ref(),
            env,
            info,
            position,
            Action::OpenPosition { data },
            assets,
        )?),
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
            deps, env, info, position, action, assets,
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

fn create_execute_strategy_messages(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    position: Position,
    action: Action,
    assets: Vec<Asset>,
) -> StdResult<Vec<CosmosMsg>> {
    let strategy_location =
        POSITION_TO_STRATEGY_LOCATION_MAP.load(deps.storage, get_position_key(&position))?;
    let strategy_id = get_terra_strategy_id(strategy_location)?;
    let manager_addr = STRATEGY_ID_TO_METADATA_MAP
        .load(deps.storage, get_strategy_id_key(strategy_id))?
        .manager_addr;
    let mut messages: Vec<CosmosMsg> = vec![];

    // Transfer assets to strategy position manager.
    let mut funds: Vec<Coin> = vec![];
    let mut assets_after_tax_deduction: Vec<Asset> = vec![];
    for asset in assets.iter() {
        match &asset.info {
            AssetInfo::NativeToken { .. } => {
                // Make sure that the message carries enough native tokens.
                asset.assert_sent_native_token_balance(&info)?;

                // Deduct tax.
                let coin_after_tax_deduction = asset.deduct_tax(&deps.querier)?;
                assets_after_tax_deduction.push(Asset {
                    info: asset.info.clone(),
                    amount: coin_after_tax_deduction.amount,
                });
                funds.push(coin_after_tax_deduction);
            }
            AssetInfo::Token { contract_addr } => {
                // Transfer this cw20 token from message sender to this contract.
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));

                // Transfer this cw20 token from this contract to strategy position manager.
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: manager_addr.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));

                // Push cw20 token asset to `assets_after_tax_deduction`.
                assets_after_tax_deduction.push(asset.clone());
            }
        }
    }

    // Ask strategy position manager to perform the requested action.
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: manager_addr.to_string(),
        msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
            position,
            action,
            assets: assets_after_tax_deduction,
        })?,
        funds,
    }));
    Ok(messages)
}
