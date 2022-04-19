use aperture_common::anchor_earn_proxy::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, PositionInfoResponse, QueryMsg,
};
use aperture_common::anchor_util::get_anchor_ust_exchange_rate;
use aperture_common::common::{get_position_key, Action, Position, Recipient};
use aperture_common::terra_manager;
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, Fraction,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::state::{
    AdminConfig, Environment, ADMIN_CONFIG, ENVIRONMENT, POSITION_TO_ANCHOR_UST_AMOUNT,
};

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
    };
    ADMIN_CONFIG.save(deps.storage, &admin_config)?;

    let environment = Environment {
        anchor_ust_cw20_addr: deps.api.addr_validate(&msg.anchor_ust_cw20_addr)?,
        anchor_market_addr: deps.api.addr_validate(&msg.anchor_market_addr)?,
    };
    ENVIRONMENT.save(deps.storage, &environment)?;

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
                Action::OpenPosition { .. } => deposit(deps, env, info, position, assets),
                Action::IncreasePosition { .. } => deposit(deps, env, info, position, assets),
                Action::DecreasePosition {
                    recipient,
                    proportion,
                } => withdraw(deps, env, &admin_config, &position, proportion, recipient),
                Action::ClosePosition { recipient } => withdraw(
                    deps,
                    env,
                    &admin_config,
                    &position,
                    Decimal::one(),
                    recipient,
                ),
            }
        }
        ExecuteMsg::UpdateAdminConfig {
            admin_addr,
            terra_manager_addr,
        } => update_admin_config(deps, info, admin_addr, terra_manager_addr),
    }
}

fn update_admin_config(
    deps: DepsMut,
    info: MessageInfo,
    admin_addr: Option<String>,
    terra_manager_addr: Option<String>,
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
    ADMIN_CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

fn deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    position: Position,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let uusd_asset = validate_assets(&info, &assets)?;

    // Obtain aUST/UST exchange rate at the current block height.
    let environment = ENVIRONMENT.load(deps.storage)?;
    let exchange_rate =
        get_anchor_ust_exchange_rate(deps.as_ref(), &env, &environment.anchor_market_addr)?;

    // Calculate the amount of aUST that will be minted for the deposited uusd amount.
    let mint_anchor_ust_amount = Uint256::from(uusd_asset.amount) / exchange_rate;

    // Update position aUST amount.
    let position_key = get_position_key(&position);
    let position_anchor_ust_amount = POSITION_TO_ANCHOR_UST_AMOUNT
        .may_load(deps.storage, position_key.clone())?
        .unwrap_or_else(Uint256::zero);
    POSITION_TO_ANCHOR_UST_AMOUNT.save(
        deps.storage,
        position_key,
        &(position_anchor_ust_amount + mint_anchor_ust_amount),
    )?;

    // Deposit uusd to Anchor Earn.
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: environment.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                amount: uusd_asset.amount,
                denom: String::from("uusd"),
            }],
        })),
    )
}

pub fn withdraw(
    deps: DepsMut,
    env: Env,
    admin_config: &AdminConfig,
    position: &Position,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    if proportion.is_zero() || proportion > Decimal::one() {
        return Err(StdError::generic_err("invalid proportion"));
    }
    let position_key = get_position_key(position);
    let anchor_ust_amount =
        POSITION_TO_ANCHOR_UST_AMOUNT.load(deps.storage, position_key.clone())?;
    let redeem_anchor_ust_amount = anchor_ust_amount
        * Decimal256::from_ratio(
            Uint256::from(proportion.numerator()),
            Uint256::from(proportion.denominator()),
        );
    if redeem_anchor_ust_amount.is_zero() || redeem_anchor_ust_amount > anchor_ust_amount {
        return Err(StdError::generic_err("invalid redeem_anchor_ust_amount"));
    }

    // Update position aUST amount.
    POSITION_TO_ANCHOR_UST_AMOUNT.save(
        deps.storage,
        position_key,
        &(anchor_ust_amount - redeem_anchor_ust_amount),
    )?;

    // Calculate uusd redemption value.
    let environment = ENVIRONMENT.load(deps.storage)?;
    let exchange_rate =
        get_anchor_ust_exchange_rate(deps.as_ref(), &env, &environment.anchor_market_addr)?;
    let withdrawal_uusd_amount = Uint128::from(redeem_anchor_ust_amount * exchange_rate);

    // Redeem aUST for uusd and disburse uusd to the recipient.
    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: environment.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: environment.anchor_market_addr.to_string(),
                amount: redeem_anchor_ust_amount.into(),
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {})?,
            })?,
            funds: vec![],
        }))
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: admin_config.terra_manager.to_string(),
            msg: to_binary(&terra_manager::ExecuteMsg::InitiateOutgoingTokenTransfer {
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                    amount: withdrawal_uusd_amount,
                }],
                recipient,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: withdrawal_uusd_amount,
            }],
        })))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position } => {
            let anchor_ust_amount = POSITION_TO_ANCHOR_UST_AMOUNT
                .may_load(deps.storage, get_position_key(&position))?
                .unwrap_or_else(Uint256::zero);
            let environment = ENVIRONMENT.load(deps.storage)?;
            to_binary(&PositionInfoResponse {
                uusd_value: anchor_ust_amount
                    * get_anchor_ust_exchange_rate(deps, &env, &environment.anchor_market_addr)?,
            })
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

// Check that `assets` comprise exactly one native-uusd asset.
fn validate_assets(info: &MessageInfo, assets: &[Asset]) -> StdResult<Asset> {
    if assets.len() == 1 {
        let asset = &assets[0];
        if let AssetInfo::NativeToken { denom } = &asset.info {
            if denom == "uusd"
                && asset.amount > Uint128::zero()
                && asset.assert_sent_native_token_balance(info).is_ok()
            {
                return Ok(asset.clone());
            }
        }
    }
    Err(StdError::GenericErr {
        msg: "invalid assets".to_string(),
    })
}

#[test]
fn test_contract() {
    use crate::mock_querier::custom_mock_dependencies;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::Addr;

    let mut deps = custom_mock_dependencies("anchor_market");
    let mut env = mock_env();
    env.contract.address = Addr::unchecked("this");
    let msg = InstantiateMsg {
        admin_addr: String::from("admin"),
        terra_manager_addr: String::from("manager"),
        anchor_ust_cw20_addr: String::from("anchor_ust_cw20"),
        anchor_market_addr: String::from("anchor_market"),
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
        }
    );
    assert_eq!(
        ENVIRONMENT.load(&deps.storage).unwrap(),
        Environment {
            anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
            anchor_market_addr: Addr::unchecked("anchor_market"),
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
            },
        ),
        Err(StdError::generic_err("unauthorized"))
    );

    // Validate assets.
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            mock_info("manager", &[]),
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
        Err(StdError::generic_err("invalid assets"))
    );

    // Deposit.
    let deposit_execute_response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(
            "manager",
            &[Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(110u128),
            }],
        ),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::OpenPosition { data: None },
            assets: vec![Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
                amount: Uint128::from(110u128),
            }],
        },
    )
    .unwrap();
    assert_eq!(deposit_execute_response.messages.len(), 1);
    assert_eq!(
        deposit_execute_response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("anchor_market"),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {}).unwrap(),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(110u128)
            }],
        })
    );
    assert_eq!(
        POSITION_TO_ANCHOR_UST_AMOUNT
            .load(&deps.storage, get_position_key(&position))
            .unwrap(),
        Uint256::from(100u128)
    );

    // Withdraw.
    let withdrawal_execute_response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::PerformAction {
            action: Action::DecreasePosition {
                proportion: Decimal::percent(40),
                recipient: Recipient::TerraChain {
                    recipient: String::from("terra1recipient"),
                },
            },
            position: position.clone(),
            assets: vec![],
        },
    )
    .unwrap();
    assert_eq!(withdrawal_execute_response.messages.len(), 2);
    assert_eq!(
        withdrawal_execute_response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("anchor_ust_cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: String::from("anchor_market"),
                amount: Uint128::from(40u128),
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {}).unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        withdrawal_execute_response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("manager"),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(44u128)
            }],
            msg: to_binary(&terra_manager::ExecuteMsg::InitiateOutgoingTokenTransfer {
                recipient: Recipient::TerraChain {
                    recipient: String::from("terra1recipient"),
                },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                    amount: Uint128::from(44u128)
                }],
            })
            .unwrap(),
        })
    );
    assert_eq!(
        POSITION_TO_ANCHOR_UST_AMOUNT
            .load(&deps.storage, get_position_key(&position))
            .unwrap(),
        Uint256::from(60u128)
    );
}
