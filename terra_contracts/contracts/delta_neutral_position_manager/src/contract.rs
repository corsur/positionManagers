use std::collections::HashSet;

use aperture_common::common::{
    get_position_key, get_position_key_from_tuple, Action, Position, Recipient,
};
use aperture_common::delta_neutral_position::PositionInfoResponse;
use aperture_common::delta_neutral_position_manager::{
    AdminConfig, BatchGetPositionInfoResponse, BatchGetPositionInfoResponseItem,
    CheckMirrorAssetAllowlistResponse, Context, DeltaNeutralParams, ExecuteMsg,
    FeeCollectionConfig, InstantiateMsg, InternalExecuteMsg, MigrateMsg, QueryMsg,
    ShouldCallRebalanceAndReinvestResponse,
};
use aperture_common::mirror_util::{
    get_mirror_asset_config_response, get_mirror_asset_fresh_oracle_uusd_rate,
    get_mirror_cdp_response, is_mirror_asset_delisted,
};
use aperture_common::terra_manager::TERRA_CHAIN_ID;
use aperture_common::{delta_neutral_position, terra_manager};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128,
    WasmMsg,
};
use cw_storage_plus::Item;
use protobuf::Message;
use terraswap::asset::{Asset, AssetInfo};

use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::{
    ADMIN_CONFIG, CONTEXT, FEE_COLLECTION_CONFIG, POSITION_OPEN_ALLOWED_MIRROR_ASSETS,
    POSITION_TO_CONTRACT_ADDR, SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS, TMP_POSITION,
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
        terra_manager: deps.api.addr_validate(&msg.terra_manager_addr)?,
        delta_neutral_position_code_id: msg.delta_neutral_position_code_id,
    };
    ADMIN_CONFIG.save(deps.storage, &admin_config)?;

    let context = Context {
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
        astroport_factory_addr: deps.api.addr_validate(&msg.astroport_factory_addr)?,
        collateral_ratio_safety_margin: msg.collateral_ratio_safety_margin,
        min_open_uusd_amount: msg.min_open_uusd_amount,
        min_reinvest_uusd_amount: msg.min_reinvest_uusd_amount,
    };
    CONTEXT.save(deps.storage, &context)?;

    FEE_COLLECTION_CONFIG.save(deps.storage, &msg.fee_collection_config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::PerformAction {
            position,
            action,
            assets,
        } => {
            let admin_config = ADMIN_CONFIG.load(deps.storage)?;
            if info.sender != admin_config.terra_manager {
                return Err(StdError::generic_err("unauthorized"));
            }
            match action {
                Action::OpenPosition { data } => {
                    let params: DeltaNeutralParams = from_binary(&data.unwrap())?;
                    open_position(env, info, deps.storage, position, params, assets)
                }
                Action::IncreasePosition { .. } => Err(StdError::generic_err("not supported")),
                Action::DecreasePosition { .. } => Err(StdError::generic_err("not supported")),
                Action::ClosePosition { recipient } => close_position(deps, &position, recipient),
            }
        }
        ExecuteMsg::MigratePositionContracts {
            positions,
            position_contracts,
        } => migrate_position_contracts(deps.as_ref(), positions, position_contracts),
        ExecuteMsg::UpdateAdminConfig {
            admin_addr,
            terra_manager_addr,
            delta_neutral_position_code_id,
        } => update_admin_config(
            deps,
            info,
            admin_addr,
            terra_manager_addr,
            delta_neutral_position_code_id,
        ),
        ExecuteMsg::UpdatePositionOpenMirrorAssetList {
            mirror_assets,
            allowed,
        } => update_position_open_mirror_asset_list(deps, info, mirror_assets, allowed),
        ExecuteMsg::AddShouldPreemptivelyCloseCdpMirrorAssetList { mirror_assets } => {
            add_should_preemptively_close_cdp_mirror_asset_list(deps, info, mirror_assets)
        }
        ExecuteMsg::UpdateFeeCollectionConfig {
            fee_collection_config,
        } => update_fee_collection_config(deps, info, fee_collection_config),
        ExecuteMsg::UpdateContext {
            controller,
            mirror_collateral_oracle_addr,
            mirror_oracle_addr,
            collateral_ratio_safety_margin,
            min_open_uusd_amount,
            min_reinvest_uusd_amount,
        } => update_context(
            deps,
            info,
            controller,
            mirror_collateral_oracle_addr,
            mirror_oracle_addr,
            collateral_ratio_safety_margin,
            min_open_uusd_amount,
            min_reinvest_uusd_amount,
        ),
        ExecuteMsg::Internal(internal_msg) => {
            if info.sender != env.contract.address {
                return Err(StdError::generic_err("unauthorized"));
            }
            match internal_msg {
                InternalExecuteMsg::SendOpenPositionToPositionContract {
                    position,
                    params,
                    uusd_amount,
                } => send_execute_message_to_position_contract(
                    deps.as_ref(),
                    &position,
                    delta_neutral_position::ExecuteMsg::OpenPosition { params },
                    Some(uusd_amount),
                ),
            }
        }
    }
}

fn update_admin_config(
    deps: DepsMut,
    info: MessageInfo,
    admin_addr: Option<String>,
    terra_manager_addr: Option<String>,
    delta_neutral_position_code_id: Option<u64>,
) -> StdResult<Response> {
    let mut config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }
    if let Some(admin_addr) = admin_addr {
        config.admin = deps.api.addr_validate(&admin_addr)?;
    }
    if let Some(terra_manager_addr) = terra_manager_addr {
        config.terra_manager = deps.api.addr_validate(&terra_manager_addr)?;
    }
    if let Some(delta_neutral_position_code_id) = delta_neutral_position_code_id {
        config.delta_neutral_position_code_id = delta_neutral_position_code_id;
    }
    ADMIN_CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

fn update_fee_collection_config(
    deps: DepsMut,
    info: MessageInfo,
    fee_collection_config: FeeCollectionConfig,
) -> StdResult<Response> {
    let config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }
    FEE_COLLECTION_CONFIG.save(deps.storage, &fee_collection_config)?;
    Ok(Response::default())
}

fn update_position_open_mirror_asset_list(
    deps: DepsMut,
    info: MessageInfo,
    mirror_assets: Vec<String>,
    allowed: bool,
) -> StdResult<Response> {
    let config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }
    for mirror_asset in mirror_assets {
        POSITION_OPEN_ALLOWED_MIRROR_ASSETS.save(deps.storage, mirror_asset, &allowed)?;
    }
    Ok(Response::default())
}

fn add_should_preemptively_close_cdp_mirror_asset_list(
    deps: DepsMut,
    info: MessageInfo,
    mirror_assets: Vec<String>,
) -> StdResult<Response> {
    let config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }
    for mirror_asset in mirror_assets {
        SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS.save(
            deps.storage,
            deps.api.addr_validate(&mirror_asset)?,
            &true,
        )?;
    }
    Ok(Response::default())
}

#[allow(clippy::too_many_arguments)]
fn update_context(
    deps: DepsMut,
    info: MessageInfo,
    controller: Option<String>,
    mirror_collateral_oracle_addr: Option<String>,
    mirror_oracle_addr: Option<String>,
    collateral_ratio_safety_margin: Option<Decimal>,
    min_open_uusd_amount: Option<Uint128>,
    min_reinvest_uusd_amount: Option<Uint128>,
) -> StdResult<Response> {
    let config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut context = CONTEXT.load(deps.storage)?;
    if let Some(controller) = controller {
        context.controller = deps.api.addr_validate(&controller)?;
    }
    if let Some(mirror_collateral_oracle_addr) = mirror_collateral_oracle_addr {
        context.mirror_collateral_oracle_addr =
            deps.api.addr_validate(&mirror_collateral_oracle_addr)?;
    }
    if let Some(mirror_oracle_addr) = mirror_oracle_addr {
        context.mirror_oracle_addr = deps.api.addr_validate(&mirror_oracle_addr)?;
    }
    if let Some(collateral_ratio_safety_margin) = collateral_ratio_safety_margin {
        context.collateral_ratio_safety_margin = collateral_ratio_safety_margin;
    }
    if let Some(min_open_uusd_amount) = min_open_uusd_amount {
        context.min_open_uusd_amount = min_open_uusd_amount;
    }
    if let Some(min_reinvest_uusd_amount) = min_reinvest_uusd_amount {
        context.min_reinvest_uusd_amount = min_reinvest_uusd_amount;
    }
    CONTEXT.save(deps.storage, &context)?;

    Ok(Response::default())
}

fn migrate_position_contracts(
    deps: Deps,
    positions: Vec<Position>,
    mut position_contracts: Vec<String>,
) -> StdResult<Response> {
    let new_code_id = ADMIN_CONFIG
        .load(deps.storage)?
        .delta_neutral_position_code_id;

    // This code-id item is used to query code ids stored under position contracts in a type-safe way.
    const CODE_ID: Item<u64> = Item::new("ci");

    // Position contracts being requested to migrate.
    position_contracts.extend(positions.iter().map(|position| {
        POSITION_TO_CONTRACT_ADDR
            .load(deps.storage, get_position_key(position))
            .unwrap()
            .to_string()
    }));

    // Generate messages for positions that need to be migrated.
    let msg = to_binary(&delta_neutral_position::MigrateMsg { new_code_id })?;
    let mut response = Response::new();
    for contract in position_contracts {
        // `Addr::unchecked` is used here to avoid the gas cost of address validation.
        let needs_migration = CODE_ID
            .query(&deps.querier, Addr::unchecked(&contract))
            .map_or(true, |current_code_id| current_code_id != new_code_id);
        if needs_migration {
            response = response.add_message(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: contract.to_string(),
                new_code_id,
                msg: msg.clone(),
            }));
        }
    }
    Ok(response)
}

fn send_execute_message_to_position_contract(
    deps: Deps,
    position: &Position,
    position_contract_execute_msg: aperture_common::delta_neutral_position::ExecuteMsg,
    uusd_amount: Option<Uint128>,
) -> StdResult<Response> {
    let contract_addr = POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(position))?;
    let mut funds: Vec<Coin> = vec![];
    if let Some(amount) = uusd_amount {
        funds.push(Coin {
            denom: String::from("uusd"),
            amount,
        });
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
    let context = CONTEXT.load(storage)?;
    let uusd_amount = validate_assets(&info, &context, &assets)?;

    // Check that the specified mirror asset is on the allowlist.
    if !POSITION_OPEN_ALLOWED_MIRROR_ASSETS.load(storage, params.mirror_asset_cw20_addr.clone())? {
        return Err(StdError::generic_err("mAsset not allowed"));
    }

    // Instantiate a new contract for the position.
    TMP_POSITION.save(storage, &position)?;
    let mut response = Response::new();
    response = response.add_submessage(SubMsg {
        msg: WasmMsg::Instantiate {
            admin: Some(env.contract.address.to_string()),
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
                uusd_amount,
            },
        ))?,
        funds: vec![],
    }));
    Ok(response)
}

pub fn close_position(
    deps: DepsMut,
    position: &Position,
    recipient: Recipient,
) -> StdResult<Response> {
    send_execute_message_to_position_contract(
        deps.as_ref(),
        position,
        delta_neutral_position::ExecuteMsg::ClosePosition { recipient },
        None,
    )
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
        QueryMsg::GetContext {} => to_binary(&CONTEXT.load(deps.storage)?),
        QueryMsg::GetAdminConfig {} => to_binary(&(ADMIN_CONFIG.load(deps.storage)?)),
        QueryMsg::CheckMirrorAssetAllowlist { mirror_assets } => {
            to_binary(&CheckMirrorAssetAllowlistResponse {
                allowed: mirror_assets
                    .into_iter()
                    .map(|mirror_asset| {
                        POSITION_OPEN_ALLOWED_MIRROR_ASSETS
                            .may_load(deps.storage, mirror_asset)
                            .unwrap()
                            .unwrap_or(false)
                    })
                    .collect(),
            })
        }
        QueryMsg::BatchGetPositionInfo { positions, ranges } => {
            let mut position_set = HashSet::new();
            if let Some(positions) = positions {
                for position in positions {
                    position_set.insert((position.chain_id, position.position_id.u128()));
                }
            }
            if let Some(ranges) = ranges {
                for range in ranges {
                    for position_id in range.start.u128()..range.end.u128() {
                        position_set.insert((range.chain_id, position_id));
                    }
                }
            }
            let aperture_terra_manager = ADMIN_CONFIG.load(deps.storage)?.terra_manager;
            let position_info_query_msg = &delta_neutral_position::QueryMsg::GetPositionInfo {};
            let mut response = BatchGetPositionInfoResponse { items: vec![] };
            for position in position_set {
                let contract_addr = POSITION_TO_CONTRACT_ADDR
                    .load(deps.storage, get_position_key_from_tuple(&position))?;
                let holder = if position.0 == TERRA_CHAIN_ID {
                    let terra_position_info: terra_manager::PositionInfoResponse =
                        deps.querier.query_wasm_smart(
                            aperture_terra_manager.to_string(),
                            &terra_manager::QueryMsg::GetTerraPositionInfo {
                                position_id: Uint128::from(position.1),
                            },
                        )?;
                    Some(terra_position_info.holder)
                } else {
                    None
                };
                response.items.push(BatchGetPositionInfoResponseItem {
                    position: Position {
                        chain_id: position.0,
                        position_id: Uint128::from(position.1),
                    },
                    contract: contract_addr.clone(),
                    holder,
                    info: deps
                        .querier
                        .query_wasm_smart(contract_addr, position_info_query_msg)?,
                });
            }
            to_binary(&response)
        }
        QueryMsg::ShouldCallRebalanceAndReinvest {
            position,
            mirror_asset_net_amount_tolerance_ratio,
            liquid_uusd_threshold_ratio,
        } => query_should_call_rebalance_and_reinvest(
            deps,
            position,
            mirror_asset_net_amount_tolerance_ratio,
            liquid_uusd_threshold_ratio,
        ),
    }
}

fn query_should_call_rebalance_and_reinvest(
    deps: Deps,
    position: Position,
    mirror_asset_net_amount_tolerance_ratio: Decimal,
    liquid_uusd_threshold_ratio: Decimal,
) -> StdResult<Binary> {
    let position_contract =
        POSITION_TO_CONTRACT_ADDR.load(deps.storage, get_position_key(&position))?;
    let position_info: PositionInfoResponse = deps.querier.query_wasm_smart(
        &position_contract,
        &delta_neutral_position::QueryMsg::GetPositionInfo {},
    )?;

    if position_info.position_close_info.is_some() {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: false,
            reason: Some(String::from("POSITION_CLOSED")),
        });
    }

    if position_info
        .detailed_info
        .as_ref()
        .unwrap()
        .cdp_preemptively_closed
    {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: false,
            reason: Some(String::from("CDP_PREEMPTIVELY_CLOSED")),
        });
    }

    let context = CONTEXT.load(deps.storage)?;
    if let Some(cdp_idx) = position_info.cdp_idx {
        if get_mirror_cdp_response(&deps.querier, &context, cdp_idx).is_err() {
            return to_binary(&ShouldCallRebalanceAndReinvestResponse {
                position_contract,
                should_call: true,
                reason: Some(String::from("LIKELY_FULL_LIQUIDATION")),
            });
        }
    }

    let fresh_oracle_uusd_rate = get_mirror_asset_fresh_oracle_uusd_rate(
        &deps.querier,
        &context,
        &position_info.mirror_asset_cw20_addr,
    );
    if fresh_oracle_uusd_rate.is_none() {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: false,
            reason: Some(String::from("ORACLE_PRICE_STALE")),
        });
    }

    if position_info.cdp_idx.is_none() {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("DELAYED_DN_OPEN")),
        });
    }

    let mirror_asset_config_response = get_mirror_asset_config_response(
        &deps.querier,
        &context.mirror_mint_addr,
        position_info.mirror_asset_cw20_addr.as_str(),
    )?;
    let should_close_cdp = SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS
        .may_load(deps.storage, position_info.mirror_asset_cw20_addr.clone())?
        == Some(true)
        || is_mirror_asset_delisted(&mirror_asset_config_response);
    if should_close_cdp {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("PREEMPTIVELY_CLOSE_CDP")),
        });
    }

    let info = &position_info.detailed_info.unwrap();
    if mirror_asset_config_response.min_collateral_ratio + context.collateral_ratio_safety_margin
        > info.target_collateral_ratio_range.min
    {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("RAISE_TARGET_CR_RANGE")),
        });
    }

    if info.collateral_ratio.unwrap() < info.target_collateral_ratio_range.min {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("CR_BELOW_MIN")),
        });
    }

    let uusd_short_proceeds_pending_unlock = !info.unclaimed_short_proceeds_uusd_amount.is_zero()
        && info.claimable_short_proceeds_uusd_amount.is_zero();
    if info.collateral_ratio.unwrap() > info.target_collateral_ratio_range.max
        && !uusd_short_proceeds_pending_unlock
    {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("CR_ABOVE_MAX")),
        });
    }

    let long_amount = info.state.as_ref().unwrap().mirror_asset_long_amount;
    let short_amount = info.state.as_ref().unwrap().mirror_asset_short_amount;
    let diff_amount = if long_amount > short_amount {
        long_amount - short_amount
    } else {
        short_amount - long_amount
    };
    if diff_amount > long_amount * mirror_asset_net_amount_tolerance_ratio {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("DELTA_ABOVE_THRESHOLD")),
        });
    }

    let liquid_uusd_amount = info.state.as_ref().unwrap().uusd_balance
        + info.claimable_mir_reward_uusd_value
        + info.claimable_spec_reward_uusd_value
        + info.claimable_short_proceeds_uusd_amount;
    if liquid_uusd_amount > info.uusd_value * liquid_uusd_threshold_ratio
        && !uusd_short_proceeds_pending_unlock
    {
        return to_binary(&ShouldCallRebalanceAndReinvestResponse {
            position_contract,
            should_call: true,
            reason: Some(String::from("LIQUID_UUSD_ABOVE_THRESHOLD")),
        });
    }

    to_binary(&ShouldCallRebalanceAndReinvestResponse {
        position_contract,
        should_call: false,
        reason: Some(String::from("LOOKS_GOOD")),
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    FEE_COLLECTION_CONFIG.save(deps.storage, &msg.fee_collection_config)?;
    for mirror_asset in msg.position_open_allowed_mirror_assets {
        POSITION_OPEN_ALLOWED_MIRROR_ASSETS.save(deps.storage, mirror_asset, &true)?;
    }
    Ok(Response::default())
}

// Check that `assets` comprise exactly one native-uusd asset of amount >= min_uusd_amount.
fn validate_assets(info: &MessageInfo, context: &Context, assets: &[Asset]) -> StdResult<Uint128> {
    if assets.len() == 1 {
        let asset = &assets[0];
        if let AssetInfo::NativeToken { denom } = &asset.info {
            if denom == "uusd"
                && asset.amount >= context.min_open_uusd_amount
                && asset.assert_sent_native_token_balance(info).is_ok()
            {
                return Ok(asset.amount);
            }
        }
    }
    Err(StdError::generic_err("invalid assets"))
}

#[test]
fn test_contract() {
    use aperture_common::delta_neutral_position_manager::FeeCollectionConfig;
    use cosmwasm_std::testing::MOCK_CONTRACT_ADDR;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Addr, Decimal};

    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let msg = InstantiateMsg {
        admin_addr: String::from("admin"),
        terra_manager_addr: String::from("manager"),
        anchor_ust_cw20_addr: String::from("anchor_ust_cw20"),
        anchor_market_addr: String::from("anchor_market"),
        delta_neutral_position_code_id: 123,
        controller: String::from("controller"),
        mirror_cw20_addr: String::from("mirror_cw20"),
        spectrum_cw20_addr: String::from("spectrum_cw20"),
        mirror_collateral_oracle_addr: String::from("mirror_collateral_oracle"),
        mirror_lock_addr: String::from("mirror_lock"),
        mirror_mint_addr: String::from("mirror_mint"),
        mirror_oracle_addr: String::from("mirror_oracle"),
        mirror_staking_addr: String::from("mirror_staking"),
        spectrum_gov_addr: String::from("spectrum_gov"),
        spectrum_mirror_farms_addr: String::from("spectrum_mirror_farms"),
        spectrum_staker_addr: String::from("spectrum_staker"),
        terraswap_factory_addr: String::from("terraswap_factory"),
        astroport_factory_addr: String::from("astroport_factory"),
        collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
        min_open_uusd_amount: Uint128::from(500u128),
        min_reinvest_uusd_amount: Uint128::from(10u128),
        fee_collection_config: FeeCollectionConfig {
            performance_rate: Decimal::from_ratio(1u128, 10u128),
            off_market_position_open_service_fee_uusd: Uint128::zero(),
            collector_addr: String::from("collector"),
        },
    };

    // Check state after instantiate().
    assert_eq!(
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("instantiate_sender", &[]),
            msg,
        )
        .unwrap()
        .messages,
        vec![]
    );
    assert_eq!(
        ADMIN_CONFIG.load(&deps.storage).unwrap(),
        AdminConfig {
            admin: Addr::unchecked("admin"),
            terra_manager: Addr::unchecked("manager"),
            delta_neutral_position_code_id: 123
        }
    );
    assert_eq!(
        CONTEXT.load(&deps.storage).unwrap(),
        Context {
            controller: Addr::unchecked("controller"),
            anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
            mirror_cw20_addr: Addr::unchecked("mirror_cw20"),
            spectrum_cw20_addr: Addr::unchecked("spectrum_cw20"),
            anchor_market_addr: Addr::unchecked("anchor_market"),
            mirror_collateral_oracle_addr: Addr::unchecked("mirror_collateral_oracle"),
            mirror_lock_addr: Addr::unchecked("mirror_lock"),
            mirror_mint_addr: Addr::unchecked("mirror_mint"),
            mirror_oracle_addr: Addr::unchecked("mirror_oracle"),
            mirror_staking_addr: Addr::unchecked("mirror_staking"),
            spectrum_gov_addr: Addr::unchecked("spectrum_gov"),
            spectrum_mirror_farms_addr: Addr::unchecked("spectrum_mirror_farms"),
            spectrum_staker_addr: Addr::unchecked("spectrum_staker"),
            terraswap_factory_addr: Addr::unchecked("terraswap_factory"),
            astroport_factory_addr: Addr::unchecked("astroport_factory"),
            collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
            min_open_uusd_amount: Uint128::from(500u128),
            min_reinvest_uusd_amount: Uint128::from(10u128),
        }
    );
    assert_eq!(
        FEE_COLLECTION_CONFIG.load(&deps.storage).unwrap(),
        FeeCollectionConfig {
            performance_rate: Decimal::from_ratio(1u128, 10u128),
            off_market_position_open_service_fee_uusd: Uint128::zero(),
            collector_addr: String::from("collector"),
        }
    );

    let position = Position {
        chain_id: 0u16,
        position_id: Uint128::zero(),
    };

    // ACL check.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("non-manager", &[]),
            ExecuteMsg::PerformAction {
                position: position.clone(),
                action: Action::OpenPosition { data: None },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(100u128),
                }]
            },
        ),
        Err(StdError::generic_err("unauthorized"))
    );
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("non-admin", &[]),
            ExecuteMsg::UpdateAdminConfig {
                admin_addr: Some(String::from("new-admin")),
                terra_manager_addr: None,
                delta_neutral_position_code_id: Some(159),
            },
        ),
        Err(StdError::generic_err("unauthorized"))
    );
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("non-admin", &[]),
            ExecuteMsg::UpdateFeeCollectionConfig {
                fee_collection_config: FeeCollectionConfig {
                    performance_rate: Decimal::from_ratio(1u128, 10u128),
                    off_market_position_open_service_fee_uusd: Uint128::from(10u128),
                    collector_addr: String::from("collector"),
                }
            },
        ),
        Err(StdError::generic_err("unauthorized"))
    );
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("non-admin", &[]),
            ExecuteMsg::UpdatePositionOpenMirrorAssetList {
                mirror_assets: vec![],
                allowed: true
            },
        ),
        Err(StdError::generic_err("unauthorized"))
    );

    let delta_neutral_params = DeltaNeutralParams {
        target_min_collateral_ratio: Decimal::from_ratio(23u128, 10u128),
        target_max_collateral_ratio: Decimal::from_ratio(27u128, 10u128),
        mirror_asset_cw20_addr: String::from("terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"),
        allow_off_market_position_open: None,
    };
    let data = Some(to_binary(&delta_neutral_params).unwrap());

    // Validate assets: check that uusd coin is sent to us.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::PerformAction {
                position: position.clone(),
                action: Action::OpenPosition { data: data.clone() },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(100u128),
                }]
            },
        ),
        Err(StdError::generic_err("invalid assets"))
    );

    // Validate assets: check that uusd amount meets the required minimum.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                "manager",
                &[Coin {
                    denom: String::from("uusd"),
                    amount: Uint128::from(100u128),
                }]
            ),
            ExecuteMsg::PerformAction {
                position: position.clone(),
                action: Action::OpenPosition { data: data.clone() },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(100u128),
                }]
            },
        ),
        Err(StdError::generic_err("invalid assets"))
    );

    // Open position with disallowed mAsset.
    assert!(execute(
        deps.as_mut(),
        env.clone(),
        mock_info(
            "manager",
            &[Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(500u128),
            }],
        ),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::OpenPosition { data: data.clone() },
            assets: vec![Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
                amount: Uint128::from(500u128),
            }],
        },
    )
    .is_err());

    // Allow mAsset and open position.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("admin", &[]),
            ExecuteMsg::UpdatePositionOpenMirrorAssetList {
                mirror_assets: vec![delta_neutral_params.mirror_asset_cw20_addr.clone()],
                allowed: true
            },
        ),
        Ok(Response::default())
    );
    let response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(
            "manager",
            &[Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(500u128),
            }],
        ),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::OpenPosition { data: data.clone() },
            assets: vec![Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
                amount: Uint128::from(500u128),
            }],
        },
    )
    .unwrap();
    assert_eq!(response.messages.len(), 2);
    assert_eq!(
        response.messages[0],
        SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: Some(MOCK_CONTRACT_ADDR.to_string()),
                code_id: 123,
                msg: to_binary(&aperture_common::delta_neutral_position::InstantiateMsg {})
                    .unwrap(),
                funds: vec![],
                label: String::new(),
            }),
            id: INSTANTIATE_REPLY_ID,
            gas_limit: None,
            reply_on: ReplyOn::Success,
        }
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: MOCK_CONTRACT_ADDR.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::Internal(
                InternalExecuteMsg::SendOpenPositionToPositionContract {
                    position: position.clone(),
                    params: delta_neutral_params,
                    uusd_amount: Uint128::from(500u128),
                },
            ))
            .unwrap(),
        })
    );
    assert_eq!(TMP_POSITION.load(deps.as_ref().storage).unwrap(), position);

    // Increase position.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info(
                "manager",
                &[Coin {
                    denom: String::from("uusd"),
                    amount: Uint128::from(600u128),
                }]
            ),
            ExecuteMsg::PerformAction {
                position: position.clone(),
                action: Action::IncreasePosition { data: data.clone() },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(600u128),
                }]
            },
        ),
        Err(StdError::generic_err("not supported"))
    );

    // Decrease position.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
            ExecuteMsg::PerformAction {
                position: position.clone(),
                action: Action::DecreasePosition {
                    proportion: Decimal::from_ratio(1u128, 3u128),
                    recipient: Recipient::TerraChain {
                        recipient: String::from("terra1recipient"),
                    }
                },
                assets: vec![]
            },
        ),
        Err(StdError::generic_err("not supported"))
    );

    // Close position.
    POSITION_TO_CONTRACT_ADDR
        .save(
            deps.as_mut().storage,
            get_position_key(&position),
            &Addr::unchecked("position_contract"),
        )
        .unwrap();
    let response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::ClosePosition {
                recipient: Recipient::TerraChain {
                    recipient: String::from("terra1recipient"),
                },
            },
            assets: vec![],
        },
    )
    .unwrap();
    assert_eq!(response.messages.len(), 1);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("position_contract"),
            funds: vec![],
            msg: to_binary(&delta_neutral_position::ExecuteMsg::ClosePosition {
                recipient: Recipient::TerraChain {
                    recipient: String::from("terra1recipient"),
                },
            })
            .unwrap(),
        })
    );

    // Admin config update.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("admin", &[]),
            ExecuteMsg::UpdateAdminConfig {
                admin_addr: Some(String::from("new-admin")),
                terra_manager_addr: None,
                delta_neutral_position_code_id: Some(165),
            },
        ),
        Ok(Response::default())
    );
    assert_eq!(
        ADMIN_CONFIG.load(deps.as_ref().storage).unwrap(),
        AdminConfig {
            admin: Addr::unchecked("new-admin"),
            terra_manager: Addr::unchecked("manager"),
            delta_neutral_position_code_id: 165,
        }
    );

    // Fee collection config update.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("new-admin", &[]),
            ExecuteMsg::UpdateFeeCollectionConfig {
                fee_collection_config: FeeCollectionConfig {
                    performance_rate: Decimal::from_ratio(2u128, 10u128),
                    off_market_position_open_service_fee_uusd: Uint128::from(100u128),
                    collector_addr: String::from("new_collector"),
                }
            },
        ),
        Ok(Response::default())
    );
    assert_eq!(
        FEE_COLLECTION_CONFIG.load(deps.as_ref().storage).unwrap(),
        FeeCollectionConfig {
            performance_rate: Decimal::from_ratio(2u128, 10u128),
            off_market_position_open_service_fee_uusd: Uint128::from(100u128),
            collector_addr: String::from("new_collector"),
        }
    );

    // Migrate position contract.
    let response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("anyone", &[]),
        ExecuteMsg::MigratePositionContracts {
            positions: vec![position.clone()],
            position_contracts: vec![String::from("terra1pos345"), String::from("terra1pos456")],
        },
    )
    .unwrap();
    assert_eq!(response.messages.len(), 3);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: String::from("terra1pos345"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg { new_code_id: 165 }).unwrap(),
        })
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: String::from("terra1pos456"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg { new_code_id: 165 }).unwrap(),
        })
    );
    assert_eq!(
        response.messages[2].msg,
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: String::from("position_contract"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg { new_code_id: 165 }).unwrap(),
        })
    );
}
