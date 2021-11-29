use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply,
    ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128, WasmMsg,
};

use protobuf::Message;

use crate::state::*;
use aperture_common::common::{DeltaNeutralParams, StrategyAction, TokenInfo};
use aperture_common::delta_neutral_position_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};

use crate::response::MsgInstantiateContractResponse;

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config { owner: info.sender };
    write_config(deps.storage, &config)?;
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
    let config = read_config(deps.storage)?;
    let is_authorized = info.sender == config.owner;
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
    write_params(storage, &params)?;
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
            code_id: 123,
            msg: to_binary(&aperture_common::delta_neutral_position::InstantiateMsg {
                controller: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g".to_string(),
                anchor_ust_cw20_addr: "terra1hzh9vpxhsk8253se0vv5jj6etdvxu3nv8z07zu".to_string(),
                mirror_cw20_addr: "terra15gwkyepfc6xgca5t5zefzwy42uts8l2m4g40k6".to_string(),
                spectrum_cw20_addr: "terra1s5eczhe0h0jutf46re52x5z4r03c8hupacxmdr".to_string(),
                anchor_market_addr: "terra1sepfj7s0aeg5967uxnfk4thzlerrsktkpelm5s".to_string(),
                mirror_collateral_oracle_addr: "terra1pmlh0j5gpzh2wsmyd3cuk39cgh2gfwk6h5wy9j"
                    .to_string(),
                mirror_lock_addr: "terra169urmlm8wcltyjsrn7gedheh7dker69ujmerv2".to_string(),
                mirror_mint_addr: "terra1wfz7h3aqf4cjmjcvc6s8lxdhh7k30nkczyf0mj".to_string(),
                mirror_oracle_addr: "terra1t6xe0txzywdg85n6k8c960cuwgh6l8esw6lau9".to_string(),
                mirror_staking_addr: "terra17f7zu97865jmknk7p2glqvxzhduk78772ezac5".to_string(),
                spectrum_gov_addr: "terra1dpe4fmcz2jqk6t50plw0gqa2q3he2tj6wex5cl".to_string(),
                spectrum_mirror_farms_addr: "terra1kehar0l76kzuvrrcwj5um72u3pjq2uvp62aruf"
                    .to_string(),
                spectrum_staker_addr: "terra1fxwelge6mf5l6z0rjpylzcfq9w9tw2q7tewaf5".to_string(),
                terraswap_factory_addr: "terra1ulgw0td86nvs4wtpsc80thv6xelk76ut7a7apj".to_string(),
            })?,
            funds: vec![],
            label: "".to_string(),
        }
        .into(),
        gas_limit: None,
        id: INSTANTIATE_REPLY_ID,
        reply_on: ReplyOn::Success,
    });

    let contract_addr = read_contract_registry(storage, params.position_id)?;
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
    let contract_addr = res.get_contract_address();
    let params = read_params(deps.storage)?;
    write_contract_registry(
        deps.storage,
        params.position_id,
        &Addr::unchecked(contract_addr),
    )?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position_id: _ } => to_binary(&(read_config(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
