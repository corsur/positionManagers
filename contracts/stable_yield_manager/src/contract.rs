use aperture_common::common::{get_position_key, Action, Position, Recipient};
use aperture_common::cross_chain_util::initiate_outgoing_token_transfers;
use aperture_common::stable_yield_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, PositionInfoResponse, QueryMsg,
};
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, Fraction,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::state::{
    AdminConfig, Environment, ShareInfo, ADMIN_CONFIG, ENVIRONMENT, POSITION_TO_SHARE_AMOUNT,
    SHARE_INFO, TOTAL_SHARE_AMOUNT,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let admin_config = AdminConfig {
        admin: deps.api.addr_validate(&msg.admin_addr)?,
        manager: deps.api.addr_validate(&msg.manager_addr)?,
        accrual_rate_per_block: msg.accrual_rate_per_block,
    };
    ADMIN_CONFIG.save(deps.storage, &admin_config)?;

    let share_info = ShareInfo {
        block_height: env.block.height,
        exchange_rate: Decimal256::one(),
    };
    SHARE_INFO.save(deps.storage, &share_info)?;
    TOTAL_SHARE_AMOUNT.save(deps.storage, &Uint256::zero())?;

    let environment = Environment {
        anchor_ust_cw20_addr: deps.api.addr_validate(&msg.anchor_ust_cw20_addr)?,
        anchor_market_addr: deps.api.addr_validate(&msg.anchor_market_addr)?,
        wormhole_token_bridge_addr: deps.api.addr_validate(&msg.wormhole_token_bridge_addr)?,
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
            if info.sender != admin_config.manager {
                return Err(StdError::generic_err("unauthorized"));
            }
            match action {
                Action::OpenPosition { .. } => {
                    deposit(deps, env, info, &admin_config, position, assets)
                }
                Action::IncreasePosition { .. } => {
                    deposit(deps, env, info, &admin_config, position, assets)
                }
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
            manager_addr,
            accrual_rate_per_block,
        } => update_admin_config(
            deps,
            info,
            env.block.height,
            admin_addr,
            manager_addr,
            accrual_rate_per_block,
        ),
    }
}

fn update_admin_config(
    mut deps: DepsMut,
    info: MessageInfo,
    block_height: u64,
    admin_addr: Option<String>,
    manager_addr: Option<String>,
    accrual_rate_per_block: Option<Decimal256>,
) -> StdResult<Response> {
    let mut config = ADMIN_CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(StdError::generic_err("unauthorized"));
    }
    if let Some(admin_addr) = admin_addr {
        config.admin = deps.api.addr_validate(&admin_addr)?;
    }
    if let Some(manager_addr) = manager_addr {
        config.manager = deps.api.addr_validate(&manager_addr)?;
    }
    if let Some(accrual_rate_per_block) = accrual_rate_per_block {
        update_uusd_value_per_share(deps.branch(), &config, block_height)?;
        config.accrual_rate_per_block = accrual_rate_per_block;
    }
    ADMIN_CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

fn deposit(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    admin_config: &AdminConfig,
    position: Position,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let uusd_asset = validate_assets(&info, &assets)?;

    // Calculate per-share value at the current block height.
    let exchange_rate = update_uusd_value_per_share(deps.branch(), admin_config, env.block.height)?;

    // Calculate the amount of share to mint for the deposited uusd amount.
    let mint_share_amount = Uint256::from(uusd_asset.amount) / exchange_rate;

    // Update total share amount.
    let total_share_amount = TOTAL_SHARE_AMOUNT.load(deps.storage)?;
    TOTAL_SHARE_AMOUNT.save(deps.storage, &(total_share_amount + mint_share_amount))?;

    // Update position share amount.
    let position_key = get_position_key(&position);
    let position_share_amount = POSITION_TO_SHARE_AMOUNT
        .may_load(deps.storage, position_key.clone())?
        .unwrap_or_else(Uint256::zero);
    POSITION_TO_SHARE_AMOUNT.save(
        deps.storage,
        position_key,
        &(position_share_amount + mint_share_amount),
    )?;

    // Deposit uusd to Anchor Earn.
    let environment = ENVIRONMENT.load(deps.storage)?;
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
    mut deps: DepsMut,
    env: Env,
    admin_config: &AdminConfig,
    position: &Position,
    proportion: Decimal,
    recipient: Recipient,
) -> StdResult<Response> {
    if proportion.is_zero() || proportion >= Decimal::one() {
        return Err(StdError::generic_err("invalid proportion"));
    }
    let position_key = get_position_key(position);
    let share_amount = POSITION_TO_SHARE_AMOUNT.load(deps.storage, position_key.clone())?;
    let burn_share_amount = share_amount
        * Decimal256::from_ratio(
            Uint256::from(proportion.numerator()),
            Uint256::from(proportion.denominator()),
        );
    if burn_share_amount.is_zero() || burn_share_amount > share_amount {
        return Err(StdError::generic_err("invalid burn_share_amount"));
    }

    // Update total share amount.
    let total_share_amount = TOTAL_SHARE_AMOUNT.load(deps.storage)?;
    TOTAL_SHARE_AMOUNT.save(deps.storage, &(total_share_amount - burn_share_amount))?;

    // Update position share amount.
    POSITION_TO_SHARE_AMOUNT.save(
        deps.storage,
        position_key,
        &(share_amount - burn_share_amount),
    )?;

    // Calculate uusd amount to withdraw.
    let exchange_rate = update_uusd_value_per_share(deps.branch(), admin_config, env.block.height)?;
    let withdrawal_uusd_amount = Uint128::from(burn_share_amount * exchange_rate);

    // Redeem aUST for uusd if necessary.
    let mut response = Response::new();
    let environment = ENVIRONMENT.load(deps.storage)?;
    let uusd_balance = terraswap::querier::query_balance(
        &deps.querier,
        env.contract.address,
        String::from("uusd"),
    )?;
    if uusd_balance < withdrawal_uusd_amount {
        // Find amount of aUST to redeem to obtain `withdrawal_uusd_amount`.
        // See https://github.com/Anchor-Protocol/money-market-contracts/blob/c85c0b8e4f7fd192504f15d7741e19da6a850f71/contracts/market/src/deposit.rs#L141
        // for details on how Anchor market contract calculates the exchange rate.
        let aterra_exchange_rate = get_aterra_exchange_rate(deps.as_ref(), &environment)?;
        let target_redemption_uusd_value = Uint256::from(withdrawal_uusd_amount - uusd_balance);
        let mut aterra_redeem_amount = target_redemption_uusd_value / aterra_exchange_rate;
        while aterra_redeem_amount * aterra_exchange_rate < target_redemption_uusd_value {
            aterra_redeem_amount += Uint256::one();
        }
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: environment.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: environment.anchor_market_addr.to_string(),
                amount: aterra_redeem_amount.into(),
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {})?,
            })?,
            funds: vec![],
        }));
    }

    // Emit messages that transfer `withdrawal_uusd_amount` uusd to the recipient.
    Ok(response.add_messages(initiate_outgoing_token_transfers(
        &environment.wormhole_token_bridge_addr,
        vec![Asset {
            info: AssetInfo::NativeToken {
                denom: String::from("uusd"),
            },
            amount: withdrawal_uusd_amount,
        }],
        recipient,
    )?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position } => {
            let share_amount = POSITION_TO_SHARE_AMOUNT
                .may_load(deps.storage, get_position_key(&position))?
                .unwrap_or_else(Uint256::zero);
            let admin_config = ADMIN_CONFIG.load(deps.storage)?;
            let share_info = SHARE_INFO.load(deps.storage)?;
            to_binary(&PositionInfoResponse {
                uusd_value: share_amount
                    * get_uusd_value_per_share(&admin_config, &share_info, env.block.height)?,
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

fn get_aterra_exchange_rate(deps: Deps, environment: &Environment) -> StdResult<Decimal256> {
    let anchor_market_state: moneymarket::market::StateResponse = deps.querier.query_wasm_smart(
        environment.anchor_market_addr.to_string(),
        &moneymarket::market::QueryMsg::State { block_height: None },
    )?;
    let anchor_market_uusd_balance = moneymarket::querier::query_balance(
        deps,
        environment.anchor_market_addr.clone(),
        String::from("uusd"),
    )?;
    let aterra_supply =
        moneymarket::querier::query_supply(deps, environment.anchor_ust_cw20_addr.clone())?;
    let aterra_exchange_rate = (Decimal256::from_uint256(anchor_market_uusd_balance)
        + anchor_market_state.total_liabilities
        - anchor_market_state.total_reserves)
        / Decimal256::from_uint256(aterra_supply);
    Ok(aterra_exchange_rate)
}

fn get_uusd_value_per_share(
    admin_config: &AdminConfig,
    share_info: &ShareInfo,
    block_height: u64,
) -> StdResult<Decimal256> {
    if share_info.block_height > block_height {
        return Err(StdError::generic_err("invalid share_info.block_height"));
    }
    Ok(share_info.exchange_rate
        * pow(
            admin_config.accrual_rate_per_block,
            block_height - share_info.block_height,
        ))
}

fn update_uusd_value_per_share(
    deps: DepsMut,
    admin_config: &AdminConfig,
    block_height: u64,
) -> StdResult<Decimal256> {
    let mut share_info = SHARE_INFO.load(deps.storage)?;
    share_info.exchange_rate = get_uusd_value_per_share(admin_config, &share_info, block_height)?;
    share_info.block_height = block_height;
    SHARE_INFO.save(deps.storage, &share_info)?;
    Ok(share_info.exchange_rate)
}

fn pow(mut x: Decimal256, mut y: u64) -> Decimal256 {
    let mut p = Decimal256::one();
    while y > 0 {
        if (y & 1) == 1 {
            p = p * x;
        }
        x = x * x;
        y >>= 1;
    }
    p
}

#[test]
fn test_pow() {
    use std::str::FromStr;
    assert_eq!(pow(Decimal256::one(), 10247682u64), Decimal256::one());
    assert_eq!(
        pow(
            Decimal256::from_ratio(Uint256::from(2u128), Uint256::one()),
            0u64
        ),
        Decimal256::one()
    );
    assert_eq!(
        pow(
            Decimal256::from_ratio(Uint256::from(2u128), Uint256::one()),
            31u64
        ),
        Decimal256::from_uint256(Uint256::from(2147483648u128))
    );
    assert_eq!(
        pow(
            Decimal256::from_ratio(Uint256::from(11u128), Uint256::from(10u128)),
            2u64
        ),
        Decimal256::from_str("1.21").unwrap()
    );
}

#[test]
fn test_contract() {
    use crate::mock_querier::custom_mock_dependencies;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{from_binary, Addr, BankMsg};
    use std::str::FromStr;

    let mut deps = custom_mock_dependencies("anchor_market", "anchor_ust_cw20");
    let mut env = mock_env();
    env.block.height = 0;
    env.contract.address = Addr::unchecked("this");
    let accrual_rate_per_block =
        Decimal256::from_ratio(Uint256::from(11u128), Uint256::from(10u128));
    let msg = InstantiateMsg {
        admin_addr: String::from("admin"),
        manager_addr: String::from("manager"),
        accrual_rate_per_block: accrual_rate_per_block.clone(),
        anchor_ust_cw20_addr: String::from("anchor_ust_cw20"),
        anchor_market_addr: String::from("anchor_market"),
        wormhole_token_bridge_addr: String::from("wormhole_token_bridge"),
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
            manager: Addr::unchecked("manager"),
            accrual_rate_per_block,
        }
    );
    assert_eq!(
        SHARE_INFO.load(&deps.storage).unwrap(),
        ShareInfo {
            exchange_rate: Decimal256::one(),
            block_height: 0,
        }
    );
    assert_eq!(
        TOTAL_SHARE_AMOUNT.load(&deps.storage).unwrap(),
        Uint256::zero()
    );
    assert_eq!(
        ENVIRONMENT.load(&deps.storage).unwrap(),
        Environment {
            anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
            anchor_market_addr: Addr::unchecked("anchor_market"),
            wormhole_token_bridge_addr: Addr::unchecked("wormhole_token_bridge"),
        }
    );

    let position = Position {
        chain_id: 0u32,
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
                manager_addr: None,
                accrual_rate_per_block: None,
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
    env.block.height = 0u64;
    let deposit_execute_response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(
            "manager",
            &[Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(100u128),
            }],
        ),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::OpenPosition { data: None },
            assets: vec![Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
                amount: Uint128::from(100u128),
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
                amount: Uint128::from(100u128)
            }],
        })
    );
    assert_eq!(
        SHARE_INFO.load(&deps.storage).unwrap(),
        ShareInfo {
            exchange_rate: Decimal256::one(),
            block_height: 0,
        }
    );
    // Exchange rate = 1.
    // 100 uusd / 1 = 100 shares.
    assert_eq!(
        TOTAL_SHARE_AMOUNT.load(&deps.storage).unwrap(),
        Uint256::from(100u128)
    );
    assert_eq!(
        POSITION_TO_SHARE_AMOUNT
            .load(&deps.storage, get_position_key(&position))
            .unwrap(),
        Uint256::from(100u128)
    );

    // Deposit after 2 blocks.
    env.block.height = 2u64;
    let deposit_execute_response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(
            "manager",
            &[Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(100u128),
            }],
        ),
        ExecuteMsg::PerformAction {
            position: position.clone(),
            action: Action::IncreasePosition { data: None },
            assets: vec![Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
                amount: Uint128::from(100u128),
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
                amount: Uint128::from(100u128)
            }],
        })
    );
    assert_eq!(
        SHARE_INFO.load(&deps.storage).unwrap(),
        ShareInfo {
            exchange_rate: Decimal256::from_str("1.21").unwrap(),
            block_height: 2,
        }
    );
    // Exchange rate = 1.21
    // floor(100 uusd / 1.21) = 82 shares.
    assert_eq!(
        TOTAL_SHARE_AMOUNT.load(&deps.storage).unwrap(),
        Uint256::from(182u128)
    );
    assert_eq!(
        POSITION_TO_SHARE_AMOUNT
            .load(&deps.storage, get_position_key(&position))
            .unwrap(),
        Uint256::from(182u128)
    );

    // Update accrual rate at block height 5.
    env.block.height = 5;
    let update_admin_config_execute_response = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("admin", &[]),
        ExecuteMsg::UpdateAdminConfig {
            admin_addr: None,
            manager_addr: None,
            accrual_rate_per_block: Some(Decimal256::from_str("1.01").unwrap()),
        },
    )
    .unwrap();
    assert_eq!(update_admin_config_execute_response.messages, vec![]);
    assert_eq!(
        SHARE_INFO.load(&deps.storage).unwrap(),
        ShareInfo {
            exchange_rate: Decimal256::from_str("1.61051").unwrap(),
            block_height: 5,
        }
    );

    // Query position info at block height 7.
    env.block.height = 7;
    let position_info_response: PositionInfoResponse = from_binary(
        &query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GetPositionInfo {
                position: position.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        position_info_response,
        PositionInfoResponse {
            // Exchange rate = 1.61051 * 1.01 * 1.01 = 1.642881251.
            // floor(182 shares * 1.642881251) = 299 uusd.
            uusd_value: Uint256::from(299u128),
        }
    );

    // Withdrawal at block height 8.
    env.block.height = 8;
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
    assert_eq!(
        SHARE_INFO.load(&deps.storage).unwrap(),
        ShareInfo {
            exchange_rate: Decimal256::from_str("1.65931006351").unwrap(),
            block_height: 8,
        }
    );
    assert_eq!(withdrawal_execute_response.messages.len(), 2);
    assert_eq!(
        withdrawal_execute_response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("anchor_ust_cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: String::from("anchor_market"),
                // floor(118 shares * 0.4 proportion) = 72 shares.
                // floor(72 * 1.65931006351) = 119 uusd.
                // Contract already has an uusd balance of 29, so target redemption uusd value is 90.
                // aUST exchange rate = 1.1 (see mock_querier).
                // aUST to withdraw = ceiling(90 / 1.1) = 82.
                amount: Uint128::from(82u128),
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {}).unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        withdrawal_execute_response.messages[1].msg,
        CosmosMsg::Bank(BankMsg::Send {
            to_address: String::from("terra1recipient"),
            // floor(118 shares * 0.4 proportion) = 72 shares.
            // floor(72 * 1.65931006351) = 119 uusd.
            amount: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(119u128)
            }]
        })
    );
}
