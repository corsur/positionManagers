use aperture_common::common::{get_position_key, Position};
use aperture_common::delta_neutral_position;
use aperture_common::delta_neutral_position_manager::{
    Action, ActionData, Context, DeltaNeutralParams, ExecuteMsg, InstantiateMsg,
    InternalExecuteMsg, MigrateMsg, QueryMsg,
};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, WasmMsg,
};
use protobuf::Message;
use terraswap::asset::{Asset, AssetInfo};

use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::{Config, CONFIG, POSITION_TO_CONTRACT_ADDR, TMP_POSITION};

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    // Store contextual informaiton.
    let config = Config {
        owner: info.sender,
        delta_neutral_position_code_id: msg.delta_neutral_position_code_id,
        min_uusd_amount: msg.min_uusd_amount,
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
        },
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

/// Dispatch enum message to its corresponding functions.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let owner = CONFIG.load(deps.storage)?.owner;
    let is_authorized = match msg {
        ExecuteMsg::PerformAction { .. } => info.sender == owner,
        ExecuteMsg::Internal(_) => info.sender == env.contract.address,
    };
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "Unauthorized delta-neutral position manager call".to_string(),
        });
    }

    match msg {
        ExecuteMsg::PerformAction {
            position,
            action_data_binary,
            assets,
        } => {
            let action_data: ActionData = from_binary(&action_data_binary.unwrap())?;
            match action_data.action {
                Action::OpenPosition {} => open_position(
                    env,
                    info,
                    deps.storage,
                    position,
                    action_data.params,
                    assets,
                ),
                Action::IncreasePosition {} => {
                    increase_position(deps.as_ref(), info, position, assets)
                }
                Action::DecreasePosition { proportion } => {
                    decrease_position(deps.as_ref(), position, proportion)
                }
                Action::ClosePosition {} => close_position(deps, position),
            }
        }
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
            InternalExecuteMsg::SendOpenPositionToPositionContract {
                position,
                params,
                uusd_asset,
            } => send_execute_message_to_position_contract(
                deps.as_ref(),
                position,
                delta_neutral_position::ExecuteMsg::OpenPosition { params },
                Some(uusd_asset),
            ),
        },
    }
}

fn send_execute_message_to_position_contract(
    deps: Deps,
    position: Position,
    position_contract_execute_msg: aperture_common::delta_neutral_position::ExecuteMsg,
    uusd_asset: Option<Asset>,
) -> StdResult<Response> {
    let contract_addr =
        POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(&position))?;
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
            code_id: config.delta_neutral_position_code_id,
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
    position: Position,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let uusd_asset = validate_assets(&info, &config, &assets)?;
    send_execute_message_to_position_contract(
        deps,
        position,
        delta_neutral_position::ExecuteMsg::IncreasePosition {},
        Some(uusd_asset),
    )
}

pub fn decrease_position(
    deps: Deps,
    position: Position,
    proportion: Decimal,
) -> StdResult<Response> {
    send_execute_message_to_position_contract(
        deps,
        position,
        delta_neutral_position::ExecuteMsg::DecreasePosition {
            proportion,
            // TODO: Pass recipient from Terra manager via position managers.
            recipient: String::new(),
        },
        None,
    )
}

pub fn close_position(deps: DepsMut, position: Position) -> StdResult<Response> {
    POSITION_TO_CONTRACT_ADDR.remove(deps.storage, get_position_key(&position));
    decrease_position(deps.as_ref(), position, Decimal::one())
}

// To store instantiated contract address into state and initiate investment.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err(
                "MsgInstantiateContractResponse",
                "failed to parse MsgInstantiateContractResponse",
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
                && asset.amount >= config.min_uusd_amount
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
