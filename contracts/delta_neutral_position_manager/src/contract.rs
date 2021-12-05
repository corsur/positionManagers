use aperture_common::common::{get_position_key, Position};
use aperture_common::delta_neutral_position_manager::{
    Action, ActionData, Context, DeltaNeutralParams, ExecuteMsg, InstantiateMsg,
    InternalExecuteMsg, MigrateMsg, QueryMsg,
};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply,
    ReplyOn, Response, StdError, StdResult, Storage, SubMsg, WasmMsg,
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
    let is_authorized = info.sender == (CONFIG.load(deps.storage)?.owner);
    // Only Terra manager can call this contract.
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "Unauthorized".to_string(),
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
                Action::IncreasePosition {} => increase_position(),
                Action::DecreasePosition { proportion: _ } => decrease_position(),
                Action::ClosePosition {} => decrease_position(),
            }
        }
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
            InternalExecuteMsg::SendOpenPositionToPositionContract {
                position,
                params,
                uusd_asset,
            } => {
                send_open_position_to_position_contract(deps.as_ref(), position, params, uusd_asset)
            }
        },
    }
}

fn send_open_position_to_position_contract(
    deps: Deps,
    position: Position,
    params: DeltaNeutralParams,
    uusd_asset: Asset,
) -> StdResult<Response> {
    let contract_addr =
        POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(&position))?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(
                // TODO: Update delta-neutral position contract to take DeltaNeutralParams.
                &aperture_common::delta_neutral_position::ExecuteMsg::OpenPosition {
                    target_min_collateral_ratio: params.target_min_collateral_ratio,
                    target_max_collateral_ratio: params.target_max_collateral_ratio,
                    mirror_asset_cw20_addr: params.mirror_asset_cw20_addr,
                },
            )?,
            funds: vec![uusd_asset.deduct_tax(&deps.querier)?],
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

    // Check that `assets` comprise exactly one native-uusd asset of amount >= min_uusd_amount.
    let mut valid_assets = false;
    if assets.len() == 1 {
        if let AssetInfo::NativeToken { denom } = &assets[0].info {
            valid_assets = denom == "uusd"
                && assets[0].amount >= config.min_uusd_amount
                && assets[0].assert_sent_native_token_balance(&info).is_ok();
        }
    }
    if !valid_assets {
        return Err(StdError::GenericErr {
            msg: "Invalid assets".to_string(),
        });
    }

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
                uusd_asset: assets[0].clone(),
            },
        ))?,
        funds: vec![],
    }));
    Ok(response)
}

// TODO: implement the corresponding methods in the position contract.
pub fn increase_position() -> StdResult<Response> {
    Ok(Response::default())
}

pub fn decrease_position() -> StdResult<Response> {
    Ok(Response::default())
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
