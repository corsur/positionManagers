use aperture_common::common::{get_position_key, Action, Position, Recipient};
use aperture_common::delta_neutral_position;
use aperture_common::delta_neutral_position_manager::{
    AdminConfig, Context, DeltaNeutralParams, ExecuteMsg, InstantiateMsg, InternalExecuteMsg,
    MigrateMsg, QueryMsg,
};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, Storage, SubMsg, Uint128, WasmMsg,
};
use protobuf::Message;
use terraswap::asset::{Asset, AssetInfo};

use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::{
    ADMIN_CONFIG, CONTEXT, FEE_COLLECTION_CONFIG, POSITION_TO_CONTRACT_ADDR, TMP_POSITION,
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

fn migrate_position_contracts(
    deps: Deps,
    positions: Vec<Position>,
    position_contracts: Vec<String>,
) -> StdResult<Response> {
    let new_code_id = ADMIN_CONFIG
        .load(deps.storage)?
        .delta_neutral_position_code_id;
    let msg = to_binary(&delta_neutral_position::MigrateMsg {})?;
    Ok(Response::new()
        .add_messages(positions.iter().map(|position| {
            let contract = POSITION_TO_CONTRACT_ADDR
                .load(deps.storage, get_position_key(position))
                .unwrap();
            CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: contract.to_string(),
                new_code_id,
                msg: msg.clone(),
            })
        }))
        .add_messages(position_contracts.iter().map(|contract| {
            CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: contract.to_string(),
                new_code_id,
                msg: msg.clone(),
            })
        })))
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
        delta_neutral_position::ExecuteMsg::DecreasePosition {
            proportion: Decimal::one(),
            recipient,
        },
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
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
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
    use cosmwasm_std::Addr;

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
            collector_addr: String::from("collector")
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

    let delta_neutral_params = DeltaNeutralParams {
        target_min_collateral_ratio: Decimal::from_ratio(23u128, 10u128),
        target_max_collateral_ratio: Decimal::from_ratio(27u128, 10u128),
        mirror_asset_cw20_addr: String::from("terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"),
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

    // Open position.
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
            msg: to_binary(&delta_neutral_position::ExecuteMsg::DecreasePosition {
                proportion: Decimal::one(),
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
        )
        .unwrap(),
        Response::default()
    );
    assert_eq!(
        ADMIN_CONFIG.load(deps.as_ref().storage).unwrap(),
        AdminConfig {
            admin: Addr::unchecked("new-admin"),
            terra_manager: Addr::unchecked("manager"),
            delta_neutral_position_code_id: 165,
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
            contract_addr: String::from("position_contract"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg {}).unwrap(),
        })
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: String::from("terra1pos345"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg {}).unwrap(),
        })
    );
    assert_eq!(
        response.messages[2].msg,
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: String::from("terra1pos456"),
            new_code_id: 165,
            msg: to_binary(&delta_neutral_position::MigrateMsg {}).unwrap(),
        })
    );
}
