use aperture_common::common::{get_position_key, Action, Position};
use aperture_common::stable_yield_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, PositionInfoResponse, QueryMsg,
};
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    Fraction, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::state::{
    AdminConfig, Environment, ShareInfo, ADMIN_CONFIG, ENVIRONMENT, MULTIPLIER,
    POSITION_TO_SHARE_AMOUNT, SHARE_INFO, TOTAL_SHARE_AMOUNT,
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
        uusd_value_times_multiplier_per_share: Uint256::from(MULTIPLIER),
    };
    SHARE_INFO.save(deps.storage, &share_info)?;
    TOTAL_SHARE_AMOUNT.save(deps.storage, &Uint256::zero())?;

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
                } => withdraw(deps, env, &position, proportion, recipient),
                Action::ClosePosition { recipient } => {
                    withdraw(deps, env, &position, Decimal::one(), recipient)
                }
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
        accrue_interest(deps.branch(), config.accrual_rate_per_block, block_height)?;
        config.accrual_rate_per_block = accrual_rate_per_block;
    }
    ADMIN_CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

fn accrue_interest(
    deps: DepsMut,
    accrual_rate_per_block: Decimal256,
    block_height: u64,
) -> StdResult<Uint256> {
    let mut share_info = SHARE_INFO.load(deps.storage)?;
    if share_info.block_height > block_height {
        return Err(StdError::generic_err("invalid share_info.block_height"));
    } else if share_info.block_height < block_height {
        let exponent = block_height - share_info.block_height;
        for _ in 0..exponent {
            share_info.uusd_value_times_multiplier_per_share =
                share_info.uusd_value_times_multiplier_per_share * accrual_rate_per_block;
        }
        share_info.block_height = block_height;
        SHARE_INFO.save(deps.storage, &share_info)?;
    }
    Ok(share_info.uusd_value_times_multiplier_per_share)
}

fn simulate_interest_accrual(
    deps: Deps,
    accrual_rate_per_block: Decimal256,
    block_height: u64,
) -> StdResult<Uint256> {
    let share_info = SHARE_INFO.load(deps.storage)?;
    let mut uusd_value_times_multiplier_per_share =
        share_info.uusd_value_times_multiplier_per_share;
    if share_info.block_height > block_height {
        return Err(StdError::generic_err("invalid share_info.block_height"));
    } else if share_info.block_height < block_height {
        let exponent = block_height - share_info.block_height;
        for _ in 0..exponent {
            uusd_value_times_multiplier_per_share =
                uusd_value_times_multiplier_per_share * accrual_rate_per_block;
        }
    }
    Ok(uusd_value_times_multiplier_per_share)
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
    let uusd_value_times_multiplier_per_share = accrue_interest(
        deps.branch(),
        admin_config.accrual_rate_per_block,
        env.block.height,
    )?;

    // Calculate the amount of share to mint for the deposited uusd amount.
    let mint_share_amount = Uint256::from(uusd_asset.amount).multiply_ratio(
        Uint256::from(MULTIPLIER),
        uusd_value_times_multiplier_per_share,
    );

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
    position: &Position,
    proportion: Decimal,
    recipient: String,
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
    POSITION_TO_SHARE_AMOUNT.save(deps.storage, position_key, &(share_amount - burn_share_amount))?;

    let admin_config = ADMIN_CONFIG.load(deps.storage)?;
    let uusd_value_times_multiplier_per_share = accrue_interest(
        deps.branch(),
        admin_config.accrual_rate_per_block,
        env.block.height,
    )?;
    let withdrawal_uusd_amount = burn_share_amount.multiply_ratio(
        uusd_value_times_multiplier_per_share,
        Uint256::from(MULTIPLIER),
    );

    // Find amount of aUST to redeem to obtain `withdrawal_uusd_amount`.
    // See https://github.com/Anchor-Protocol/money-market-contracts/blob/c85c0b8e4f7fd192504f15d7741e19da6a850f71/contracts/market/src/deposit.rs#L141
    // for details on how Anchor market contract calculates the exchange rate.
    let environment = ENVIRONMENT.load(deps.storage)?;
    let anchor_market_state: moneymarket::market::StateResponse = deps.querier.query_wasm_smart(
        environment.anchor_market_addr.to_string(),
        &moneymarket::market::QueryMsg::State {
            block_height: Some(env.block.height),
        },
    )?;
    let anchor_market_uusd_balance = moneymarket::querier::query_balance(
        deps.as_ref(),
        environment.anchor_market_addr.clone(),
        String::from("uusd"),
    )?;
    let aterra_supply = moneymarket::querier::query_supply(
        deps.as_ref(),
        environment.anchor_ust_cw20_addr.clone(),
    )?;
    let aterra_exchange_rate = (Decimal256::from_uint256(anchor_market_uusd_balance)
        + anchor_market_state.total_liabilities
        - anchor_market_state.total_reserves)
        / Decimal256::from_uint256(aterra_supply);
    let mut aterra_redeem_amount = withdrawal_uusd_amount / aterra_exchange_rate;
    while aterra_redeem_amount * aterra_exchange_rate < withdrawal_uusd_amount {
        aterra_redeem_amount += Uint256::one();
    }

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: environment.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: environment.anchor_market_addr.to_string(),
                amount: aterra_redeem_amount.into(),
                msg: to_binary(&moneymarket::market::Cw20HookMsg::RedeemStable {})?,
            })?,
            funds: vec![],
        }))
        .add_message(BankMsg::Send {
            to_address: recipient,
            amount: vec![Coin {
                amount: withdrawal_uusd_amount.into(),
                denom: String::from("uusd"),
            }],
        }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo { position } => {
            let share_amount = POSITION_TO_SHARE_AMOUNT
                .may_load(deps.storage, get_position_key(&position))?
                .unwrap_or_else(Uint256::zero);
            let admin_config = ADMIN_CONFIG.load(deps.storage)?;
            let uusd_value_times_multiplier_per_share = simulate_interest_accrual(
                deps,
                admin_config.accrual_rate_per_block,
                env.block.height,
            )?;
            to_binary(&PositionInfoResponse {
                uusd_value: share_amount.multiply_ratio(
                    uusd_value_times_multiplier_per_share,
                    Uint256::from(MULTIPLIER),
                ),
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
        msg: "Invalid assets".to_string(),
    })
}
