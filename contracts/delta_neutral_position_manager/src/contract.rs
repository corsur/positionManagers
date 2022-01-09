use aperture_common::common::{get_position_key, Action, Position};
use aperture_common::delta_neutral_position;
use aperture_common::delta_neutral_position_manager::{
    Context, DeltaNeutralParams, ExecuteMsg, InstantiateMsg, InternalExecuteMsg, MigrateMsg,
    OpenPositionsResponse, QueryMsg,
};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, WasmMsg,
};
use cw_storage_plus::{Bound, PrimaryKey};
use protobuf::Message;
use terraswap::asset::{Asset, AssetInfo};

use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::{
    AdminConfig, Config, ADMIN_CONFIG, CONFIG, POSITION_TO_CONTRACT_ADDR, TMP_POSITION,
};

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let admin_config = AdminConfig {
        admin: deps.api.addr_validate(&msg.admin_addr)?,
        manager: deps.api.addr_validate(&msg.manager_addr)?,
        delta_neutral_position_code_id: msg.delta_neutral_position_code_id,
        allow_position_increase: msg.allow_position_increase,
        allow_position_decrease: msg.allow_position_decrease,
    };
    ADMIN_CONFIG.save(deps.storage, &admin_config)?;

    let config = Config {
        context: Context {
            controller: deps.api.addr_validate(&msg.controller)?,
            anchor_ust_cw20_addr: deps.api.addr_validate(&msg.anchor_ust_cw20_addr)?,
            mirror_cw20_addr: deps.api.addr_validate(&msg.mirror_cw20_addr)?,
            spectrum_cw20_addr: deps.api.addr_validate(&msg.spectrum_cw20_addr)?,
            anchor_market_addr: deps.api.addr_validate(&msg.anchor_market_addr)?,
            mirror_collateral_oracle_addr: deps
                .api
                .addr_validate(&msg.mirror_collateral_oracle_addr)?,
            mirror_lock_addr: deps.api.addr_validate(&msg.mirror_lock_addr)?,
            mirror_mint_addr: deps.api.addr_validate(&msg.mirror_mint_addr)?,
            mirror_oracle_addr: deps.api.addr_validate(&msg.mirror_oracle_addr)?,
            mirror_staking_addr: deps.api.addr_validate(&msg.mirror_staking_addr)?,
            spectrum_gov_addr: deps.api.addr_validate(&msg.spectrum_gov_addr)?,
            spectrum_mirror_farms_addr: deps.api.addr_validate(&msg.spectrum_mirror_farms_addr)?,
            spectrum_staker_addr: deps.api.addr_validate(&msg.spectrum_staker_addr)?,
            terraswap_factory_addr: deps.api.addr_validate(&msg.terraswap_factory_addr)?,
            collateral_ratio_safety_margin: msg.collateral_ratio_safety_margin,
            min_delta_neutral_uusd_amount: msg.min_delta_neutral_uusd_amount,
        },
        fee_collection: msg.fee_collection_config,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

/// Dispatch enum message to its corresponding functions.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::PerformAction {
            position,
            action,
            assets,
        } => {
            let admin_config = ADMIN_CONFIG.load(deps.storage)?;
            if info.sender != admin_config.manager {
                return Err(StdError::generic_err("unauthorized"));
            }
            match action {
                Action::OpenPosition { data } => {
                    let params: DeltaNeutralParams = from_binary(&data.unwrap())?;
                    open_position(env, info, deps.storage, position, params, assets)
                }
                Action::IncreasePosition { data } => {
                    if !admin_config.allow_position_increase {
                        return Err(StdError::generic_err("unauthorized"));
                    }
                    increase_position(deps.as_ref(), info, &position, assets, data)
                }
                Action::DecreasePosition {
                    proportion,
                    recipient,
                } => {
                    if !admin_config.allow_position_decrease {
                        return Err(StdError::generic_err("unauthorized"));
                    }
                    decrease_position(deps.as_ref(), &position, proportion, recipient)
                }
                Action::ClosePosition { recipient } => close_position(deps, &position, recipient),
            }
        }
        ExecuteMsg::Internal(internal_msg) => {
            if info.sender != env.contract.address {
                return Err(StdError::generic_err("unauthorized"));
            }
            match internal_msg {
                InternalExecuteMsg::SendOpenPositionToPositionContract {
                    position,
                    params,
                    uusd_asset,
                } => send_execute_message_to_position_contract(
                    deps.as_ref(),
                    &position,
                    delta_neutral_position::ExecuteMsg::OpenPosition { params },
                    Some(uusd_asset),
                ),
            }
        }
    }
}

fn send_execute_message_to_position_contract(
    deps: Deps,
    position: &Position,
    position_contract_execute_msg: aperture_common::delta_neutral_position::ExecuteMsg,
    uusd_asset: Option<Asset>,
) -> StdResult<Response> {
    let contract_addr = POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(position))?;
    let mut funds: Vec<Coin> = vec![];
    if let Some(asset) = uusd_asset {
        funds.push(asset.deduct_tax(&deps.querier)?);
    }
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&position_contract_execute_msg)?,
            funds,
        })),
    )
}

pub fn open_position(
    env: Env,
    info: MessageInfo,
    storage: &mut dyn Storage,
    position: Position,
    params: DeltaNeutralParams,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let config = CONFIG.load(storage)?;
    let uusd_asset = validate_assets(&info, &config, &assets)?;

    // Instantiate a new contract for the position.
    TMP_POSITION.save(storage, &position)?;
    let mut response = Response::new();
    response = response.add_submessage(SubMsg {
        msg: WasmMsg::Instantiate {
            admin: None,
            code_id: ADMIN_CONFIG.load(storage)?.delta_neutral_position_code_id,
            msg: to_binary(&aperture_common::delta_neutral_position::InstantiateMsg {})?,
            funds: vec![],
            label: String::new(),
        }
        .into(),
        gas_limit: None,
        id: INSTANTIATE_REPLY_ID,
        reply_on: ReplyOn::Success,
    });

    // Call position contract to open this position.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::Internal(
            InternalExecuteMsg::SendOpenPositionToPositionContract {
                position,
                params,
                uusd_asset,
            },
        ))?,
        funds: vec![],
    }));
    Ok(response)
}

pub fn increase_position(
    deps: Deps,
    info: MessageInfo,
    position: &Position,
    assets: Vec<Asset>,
    data: Option<Binary>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let uusd_asset = validate_assets(&info, &config, &assets)?;
    send_execute_message_to_position_contract(
        deps,
        position,
        delta_neutral_position::ExecuteMsg::IncreasePosition {
            ignore_uusd_pending_unlock: from_binary(&data.unwrap())?,
        },
        Some(uusd_asset),
    )
}

pub fn decrease_position(
    deps: Deps,
    position: &Position,
    proportion: Decimal,
    recipient: String,
) -> StdResult<Response> {
    if proportion <= Decimal::zero() || proportion > Decimal::one() {
        return Err(StdError::GenericErr {
            msg: "`proportion` must safity `0 < proportion <= 1`.".to_string(),
        });
    }
    send_execute_message_to_position_contract(
        deps,
        position,
        delta_neutral_position::ExecuteMsg::DecreasePosition {
            proportion,
            recipient,
        },
        None,
    )
}

pub fn close_position(
    deps: DepsMut,
    position: &Position,
    recipient: String,
) -> StdResult<Response> {
    decrease_position(deps.as_ref(), position, Decimal::one(), recipient)
}

// To store instantiated contract address into state and initiate investment.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err(
                "MsgInstantiateContractResponse",
                "Delta Neutral Position Manager failed to parse MsgInstantiateContractResponse",
            )
        })?;
    let contract_addr = deps.api.addr_validate(res.get_contract_address())?;
    let position = TMP_POSITION.load(deps.storage)?;
    POSITION_TO_CONTRACT_ADDR.save(deps.storage, get_position_key(&position), &contract_addr)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionContractAddr { position } => {
            to_binary(&(POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(&position))?))
        }
        QueryMsg::GetContext {} => to_binary(&(CONFIG.load(deps.storage)?).context),
        QueryMsg::GetOpenPositions {
            start_after,
            limit,
        } => {
            let min = start_after.map(|position| {
                Bound::Exclusive(get_position_key(&position).joined_key())
            });
            let limit = match limit {
                Some(value) => value,
                None => usize::MAX,
            };
            let positions: StdResult<Vec<_>> = POSITION_TO_CONTRACT_ADDR.range(
                deps.storage,
                min,
                None,
                cosmwasm_std::Order::Ascending,
            ).take(limit).collect();
            let mut open_positions: Vec<Addr> = vec![];
            
            for position in positions? {
                let (_, contract) = position;
                let position_info: delta_neutral_position::PositionInfoResponse =
                    deps.querier.query_wasm_smart(
                        contract.clone(),
                        &delta_neutral_position::QueryMsg::GetPositionInfo {},
                    )?;
                if position_info.position_close_block_info == None {
                    open_positions.push(contract);
                }
            }
            to_binary(&OpenPositionsResponse {
                contracts: open_positions,
            })
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

// Check that `assets` comprise exactly one native-uusd asset of amount >= min_uusd_amount.
fn validate_assets(info: &MessageInfo, config: &Config, assets: &[Asset]) -> StdResult<Asset> {
    if assets.len() == 1 {
        let asset = &assets[0];
        if let AssetInfo::NativeToken { denom } = &asset.info {
            if denom == "uusd"
                && asset.amount >= config.context.min_delta_neutral_uusd_amount
                && asset.assert_sent_native_token_balance(info).is_ok()
            {
                return Ok(asset.clone());
            }
        }
    }
    Err(StdError::GenericErr {
        msg: "Invalid assets".to_string(),
    })
}
