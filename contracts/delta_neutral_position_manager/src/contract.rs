use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply,
    ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128, WasmMsg,
};

use cw_storage_plus::U128Key;
use protobuf::Message;

use crate::state::{Config, CONFIG, POSITIONS, TMP_POSITION_ID};
use aperture_common::common::{DeltaNeutralParams, StrategyAction, TokenInfo};
use aperture_common::delta_neutral_position_manager::{
    Context, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};

use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;

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
        },
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

/// Dispatch enum message to its corresponding functions.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    let is_authorized = info.sender == (CONFIG.load(deps.storage)?.owner);
    // Only Terra manager can call this contract.
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "Unauthorized".to_string(),
        });
    }

    let ExecuteMsg::Do {
        action,
        token,
        params,
    } = msg;

    match action {
        StrategyAction::OpenPosition {} => open_position(deps.storage, token, params),
        StrategyAction::IncreasePosition {} => increase_position(token, params),
        StrategyAction::DecreasePosition {} => decrease_position(token, params),
        StrategyAction::ClosePosition {} => close_position(token, params),
    }
}

pub fn open_position(
    storage: &mut dyn Storage,
    token: TokenInfo,
    params: DeltaNeutralParams,
) -> StdResult<Response> {
    // Step 1: Instantiate a new contract for the position id.
    // Step 2: Send msg to contract to create position.
    TMP_POSITION_ID.save(storage, &params.position_id.u128())?;
    if !token.native || token.denom != "uusd" {
        return Err(StdError::GenericErr {
            msg: "Delta neutral fund input must be uusd".to_string(),
        });
    }
    let mut response = Response::new();
    response = response.add_submessage(SubMsg {
        // Create contract for the position id.
        msg: WasmMsg::Instantiate {
            admin: None,
            code_id: CONFIG.load(storage)?.delta_neutral_position_code_id,
            msg: to_binary(&aperture_common::delta_neutral_position::InstantiateMsg {})?,
            funds: vec![],
            label: String::new(),
        }
        .into(),
        gas_limit: None,
        id: INSTANTIATE_REPLY_ID,
        reply_on: ReplyOn::Success,
    });

    // TODO(lipeiqian): Move the code block below to an internal message. Currently this is executed before
    // instantiation of the position contract.
    let contract_addr = POSITIONS.load(storage, U128Key::from(params.position_id.u128()))?;
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(
            &aperture_common::delta_neutral_position::ExecuteMsg::OpenPosition {
                collateral_ratio_in_percentage: params.collateral_ratio_in_percentage,
                buffer_percentage: Uint128::from(5u128),
                mirror_asset_cw20_addr: params.mirror_asset_cw20_addr,
            },
        )?,
        funds: vec![Coin::new(token.amount.u128(), token.denom)],
    }));

    Ok(response)
}

pub fn increase_position(_token: TokenInfo, _params: DeltaNeutralParams) -> StdResult<Response> {
    Ok(Response::default())
}

pub fn decrease_position(_token: TokenInfo, _params: DeltaNeutralParams) -> StdResult<Response> {
    Ok(Response::default())
}

pub fn close_position(_token: TokenInfo, _params: DeltaNeutralParams) -> StdResult<Response> {
    Ok(Response::default())
}

// To store instantiated contract address into state and initiate investment.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;
    let contract_addr = deps.api.addr_validate(res.get_contract_address())?;
    let position_id_key = U128Key::from(TMP_POSITION_ID.load(deps.storage)?);
    POSITIONS.save(deps.storage, position_id_key, &contract_addr)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position_id: _ } => to_binary(&(CONFIG.load(deps.storage)?)),
        QueryMsg::GetContext {} => to_binary(&(CONFIG.load(deps.storage)?).context),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
