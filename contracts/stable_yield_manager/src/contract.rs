use aperture_common::common::{get_position_key, Action, Position};
use aperture_common::stable_yield_manager::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, PositionInfoResponse, QueryMsg,
};
use cosmwasm_std::{
    entry_point, to_binary, Binary, CosmosMsg, Decimal, Decimal256, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, Uint256, WasmMsg,
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
                } => withdraw(deps, &position, proportion, recipient),
                Action::ClosePosition { recipient } => {
                    withdraw(deps, &position, Decimal::one(), recipient)
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

pub fn deposit(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    admin_config: &AdminConfig,
    position: Position,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let uusd_asset = validate_assets(&info, &assets)?;
    let uusd_coin = uusd_asset.deduct_tax(&deps.querier)?;

    // Calculate per-share value at the current block height.
    let uusd_value_times_multiplier_per_share = accrue_interest(
        deps.branch(),
        admin_config.accrual_rate_per_block,
        env.block.height,
    )?;

    // Calculate the amount of share to mint for the deposited uusd amount.
    let mint_share_amount = Uint256::from(uusd_coin.amount)
        .multiply_ratio(MULTIPLIER, uusd_value_times_multiplier_per_share);

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
            funds: vec![uusd_coin],
        })),
    )
}

pub fn withdraw(
    _deps: DepsMut,
    _position: &Position,
    _proportion: Decimal,
    _recipient: String,
) -> StdResult<Response> {
    Err(StdError::generic_err("not yet implemented"))
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
                uusd_value: share_amount
                    .multiply_ratio(uusd_value_times_multiplier_per_share, MULTIPLIER),
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
