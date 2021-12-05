use aperture_common::common::{
    get_position_key, Position, Strategy, StrategyMetadata, StrategyPositionManagerExecuteMsg,
};
use aperture_common::nft::{Extension, Metadata};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, Uint64, WasmMsg,
};
use terraswap::asset::{Asset, AssetInfo};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, APERTURE_NFT, TERRA_CHAIN_ID};
use crate::state::{
    get_strategy_id_key, NEXT_POSITION_ID, NEXT_STRATEGY_ID, NFT_ADDR, OWNER,
    POSITION_TO_STRATEGY_MAP, STRATEGY_ID_TO_METADATA_MAP,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    OWNER.save(deps.storage, &info.sender)?;
    NFT_ADDR.save(deps.storage, &deps.api.addr_validate(&msg.nft_addr)?)?;
    NEXT_STRATEGY_ID.save(deps.storage, &Uint64::zero())?;
    NEXT_POSITION_ID.save(deps.storage, &Uint128::zero())?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    // TODO(lipeiqian): Move owner-only messages under a separate enum.
    let is_authorized: bool = match msg {
        ExecuteMsg::CreateTerraNFTPosition { .. } => true,
        ExecuteMsg::ExecuteStrategy { .. } => true,
        _ => info.sender == OWNER.load(deps.storage)?,
    };
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "Unauthorized".to_string(),
        });
    }
    match msg {
        ExecuteMsg::AddStrategy {
            name,
            version,
            manager_addr,
        } => add_strategy(deps, name, version, manager_addr),
        ExecuteMsg::RemoveStrategy { strategy_id } => remove_strategy(deps, strategy_id),
        ExecuteMsg::CreateTerraNFTPosition {
            strategy,
            action_data_binary,
            assets,
        } => create_terra_nft_position(deps, env, info, strategy, action_data_binary, assets),
        ExecuteMsg::ExecuteStrategy {
            position,
            action_data_binary,
            assets,
        } => execute_strategy(
            deps.as_ref(),
            env,
            info,
            position,
            action_data_binary,
            assets,
        ),
    }
}

pub fn add_strategy(
    deps: DepsMut,
    name: String,
    version: String,
    manager_addr: String,
) -> StdResult<Response> {
    let strategy_id = NEXT_STRATEGY_ID.load(deps.storage)?;
    NEXT_STRATEGY_ID.save(deps.storage, &(strategy_id.checked_add(1u64.into())?))?;
    STRATEGY_ID_TO_METADATA_MAP.save(
        deps.storage,
        get_strategy_id_key(strategy_id),
        &StrategyMetadata {
            name,
            version,
            manager_addr: deps.api.addr_validate(&manager_addr)?,
        },
    )?;
    Ok(Response::default())
}

pub fn remove_strategy(deps: DepsMut, strategy_id: Uint64) -> StdResult<Response> {
    STRATEGY_ID_TO_METADATA_MAP.remove(deps.storage, get_strategy_id_key(strategy_id));
    Ok(Response::default())
}

pub fn create_terra_nft_position(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    strategy: Strategy,
    action_data: Option<Binary>,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    // Issue CW-721 token as a receipt to the user.
    let position_id = NEXT_POSITION_ID.load(deps.storage)?;
    NEXT_POSITION_ID.save(deps.storage, &position_id.checked_add(1u128.into())?)?;
    let metadata: Extension = Some(Metadata {
        name: Some(APERTURE_NFT.to_string()),
        description: None,
    });

    let nft_mint_msg = cw721_base::ExecuteMsg::Mint(cw721_base::MintMsg {
        token_id: position_id.to_string(),
        owner: info.sender.to_string(),
        token_uri: None,
        extension: metadata,
    });

    // Update POSITION_TO_STRATEGY_MAP.
    let position = Position {
        chain_id: TERRA_CHAIN_ID,
        position_id,
    };
    POSITION_TO_STRATEGY_MAP.save(deps.storage, get_position_key(&position), &strategy)?;

    // Execute strategy.
    let response = execute_strategy(deps.as_ref(), env, info, position, action_data, assets)?;
    Ok(response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: (NFT_ADDR.load(deps.storage)?).to_string(),
        msg: to_binary(&nft_mint_msg)?,
        funds: vec![],
    })))
}

pub fn execute_strategy(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    position: Position,
    action_data_binary: Option<Binary>,
    assets: Vec<Asset>,
) -> StdResult<Response> {
    let strategy = POSITION_TO_STRATEGY_MAP.load(deps.storage, get_position_key(&position))?;
    if strategy.chain_id != TERRA_CHAIN_ID {
        return Err(StdError::GenericErr {
            msg: "Cross-chain action not yet supported".to_string(),
        });
    }

    let manager_addr = STRATEGY_ID_TO_METADATA_MAP
        .load(deps.storage, get_strategy_id_key(strategy.strategy_id))?
        .manager_addr;
    let mut response = Response::new();

    // Transfer assets to strategy position manager.
    let mut funds: Vec<Coin> = vec![];
    let mut assets_after_tax_deduction: Vec<Asset> = vec![];
    for asset in assets.iter() {
        match &asset.info {
            AssetInfo::NativeToken { .. } => {
                // Make sure that the message carries enough native tokens.
                asset.assert_sent_native_token_balance(&info)?;

                // Deduct tax.
                let coin_after_tax_deduction = asset.deduct_tax(&deps.querier)?;
                assets_after_tax_deduction.push(Asset {
                    info: asset.info.clone(),
                    amount: coin_after_tax_deduction.amount,
                });
                funds.push(coin_after_tax_deduction);
            }
            AssetInfo::Token { contract_addr } => {
                // Transfer this cw20 token from message sender to this contract.
                response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));

                // Transfer this cw20 token from this contract to strategy position manager.
                response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: manager_addr.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));

                // Push cw20 token asset to `assets_after_tax_deduction`.
                assets_after_tax_deduction.push(asset.clone());
            }
        }
    }

    // Ask strategy position manager to perform the requested action.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: manager_addr.to_string(),
        msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
            position,
            action_data_binary,
            assets: assets_after_tax_deduction,
        })?,
        funds,
    }));
    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetStrategyMetadata { strategy_id } => to_binary(
            &STRATEGY_ID_TO_METADATA_MAP.load(deps.storage, get_strategy_id_key(strategy_id))?,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
