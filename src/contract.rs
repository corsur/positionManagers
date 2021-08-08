use cosmwasm_std::{
    to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, InitResponse, LogAttribute, Querier, QueryRequest, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};
use crate::util::{*};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        owner: deps.api.canonical_address(&env.message.sender)?,
        anchor_ust_cw20_addr: deps.api.canonical_address(&msg.anchor_ust_cw20_addr)?,
        mirror_asset_cw20_addr: deps.api.canonical_address(&msg.mirror_asset_cw20_addr)?,
        mirror_collateral_oracle_addr: deps.api.canonical_address(&msg.mirror_collateral_oracle_addr)?,
        mirror_lock_addr: deps.api.canonical_address(&msg.mirror_lock_addr)?,
        mirror_mint_addr: deps.api.canonical_address(&msg.mirror_mint_addr)?,
        mirror_oracle_addr: deps.api.canonical_address(&msg.mirror_oracle_addr)?,
        mirror_staking_addr: deps.api.canonical_address(&msg.mirror_staking_addr)?,
        terraswap_factory_addr: deps.api.canonical_address(&msg.terraswap_factory_addr)?,
    };
    config(&mut deps.storage).save(&state)?;
    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;
    if deps.api.canonical_address(&env.message.sender)? != state.owner {
        return Err(StdError::unauthorized());
    }
    match msg {
        HandleMsg::ClaimShortSaleProceedsAndStake {cdp_idx} => claim_short_sale_proceeds_and_stake(deps, env, cdp_idx),
        HandleMsg::DeltaNeutralInvest {collateral_asset_amount, collateral_ratio_in_percentage} =>
            try_delta_neutral_invest(deps, env, collateral_asset_amount, collateral_ratio_in_percentage),
        HandleMsg::Do {cosmos_messages} => try_to_do(deps, env, cosmos_messages),
    }
}

pub fn try_to_do<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    cosmos_messages: Vec<CosmosMsg>,
) -> StdResult<HandleResponse> {
    Ok(HandleResponse {
        messages: cosmos_messages,
        log: vec![],
        data: None,
    })
}

pub fn try_delta_neutral_invest<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    collateral_asset_amount: Uint128,
    collateral_ratio_in_percentage: Uint128,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;
    let collateral_ratio = Decimal::from_ratio(collateral_ratio_in_percentage, 100u128);
    let inverse_collateral_ratio = Decimal::from_ratio(100u128, collateral_ratio_in_percentage);

    let collateral_price_response: mirror_protocol::collateral_oracle::CollateralPriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&state.mirror_collateral_oracle_addr)?,
            msg: to_binary(&mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
                asset: deps.api.human_address(&state.anchor_ust_cw20_addr)?.to_string(),
            })?,
        }))?;
    let collateral_value_in_uusd: Uint128 = collateral_asset_amount * collateral_price_response.rate;
    let minted_mirror_asset_value_in_uusd: Uint128 = collateral_value_in_uusd * inverse_collateral_ratio;

    let mirror_asset_oracle_price_response: mirror_protocol::oracle::PriceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(&state.mirror_oracle_addr)?,
        msg: to_binary(&mirror_protocol::oracle::QueryMsg::Price {
            base_asset: deps.api.human_address(&state.mirror_asset_cw20_addr)?.to_string(),
            quote_asset: String::from("uusd"),
        })?,
    }))?;
    let mirror_asset_oracle_price_in_uusd: Decimal = mirror_asset_oracle_price_response.rate;
    let minted_mirror_asset_amount: Uint128 = minted_mirror_asset_value_in_uusd * inverse_decimal(mirror_asset_oracle_price_in_uusd);

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(deps.api.human_address(&state.mirror_asset_cw20_addr)?);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        deps, &deps.api.human_address(&state.terraswap_factory_addr)?, &terraswap_pair_asset_info)?;
    let uusd_swap_amount = get_uusd_amount_to_swap_for_long_position(
        deps, &terraswap_pair_info.contract_addr, &terraswap_pair_asset_info[0], &terraswap_pair_asset_info[1], minted_mirror_asset_amount)?;

    let open_cdp = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.anchor_ust_cw20_addr)?,
        msg: to_binary(&cw20::Cw20HandleMsg::Send {
            contract: deps.api.human_address(&state.mirror_mint_addr)?,
            amount: collateral_asset_amount,
            msg: Some(to_binary(&mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                asset_info: terraswap::asset::AssetInfo::Token {
                    contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
                },
                collateral_ratio: collateral_ratio,
                short_params: None,
            })?),
        })?,
        send: vec![],
    });

    let swap_uusd_for_mirror_asset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair_info.contract_addr,
        msg: to_binary(&terraswap::pair::HandleMsg::Swap {
            offer_asset: terraswap::asset::Asset {
                info: terraswap_pair_asset_info[1].clone(),
                amount: uusd_swap_amount,
            },
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        send: vec![
            Coin {
                denom: String::from("uusd"),
                amount: uusd_swap_amount,
            }
        ],
    });

    let response = HandleResponse {
        messages: vec![
            open_cdp,
            swap_uusd_for_mirror_asset,
        ],
        log: vec![LogAttribute {
            key: String::from("mirror_asset_oracle_price_in_uusd"),
            value: mirror_asset_oracle_price_in_uusd.to_string(),
        }, LogAttribute {
            key: String::from("collateral_value_in_uusd"),
            value: collateral_value_in_uusd.to_string(),
        }, LogAttribute {
            key: String::from("minted_mirror_asset_value_in_uusd"),
            value: minted_mirror_asset_value_in_uusd.to_string(),
        }, LogAttribute {
            key: String::from("minted_mirror_asset_amount"),
            value: minted_mirror_asset_amount.to_string(),
        }, LogAttribute {
            key: String::from("uusd_swap_amount"),
            value: uusd_swap_amount.to_string(),
        }],
        data: None,
    };
    Ok(response)
}

pub fn claim_short_sale_proceeds_and_stake<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    cdp_idx: Uint128,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let mirror_asset_cw20_info = terraswap::asset::AssetInfo::Token {
        contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
    };
    let mirror_asset_amount = mirror_asset_cw20_info.query_pool(deps, &env.contract.address)?;
    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
        msg: to_binary(&cw20::Cw20HandleMsg::IncreaseAllowance {
            spender: deps.api.human_address(&state.mirror_staking_addr)?,
            amount: mirror_asset_amount,
            expires: None,
        })?,
        send: vec![],
    });

    let unlock_position_funds = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.mirror_lock_addr)?,
        msg: to_binary(&mirror_protocol::lock::HandleMsg::UnlockPositionFunds {
            positions_idx: vec![cdp_idx],
        })?,
        send: vec![],
    });

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(deps.api.human_address(&state.mirror_asset_cw20_addr)?);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        deps, &deps.api.human_address(&state.terraswap_factory_addr)?, &terraswap_pair_asset_info)?;
    let pool_mirror_asset_balance = terraswap_pair_asset_info[0].query_pool(deps, &terraswap_pair_info.contract_addr)?;
    let pool_uusd_balance = terraswap_pair_asset_info[1].query_pool(deps, &terraswap_pair_info.contract_addr)?;
    let uusd_amount_to_provide_liquidity = mirror_asset_amount.multiply_ratio(pool_uusd_balance, pool_mirror_asset_balance);
    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.mirror_staking_addr)?,
        msg: to_binary(&mirror_protocol::staking::HandleMsg::AutoStake {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
                    },
                    amount: mirror_asset_amount,
                },
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                    amount: uusd_amount_to_provide_liquidity,
                },
            ],
            slippage_tolerance: None,
        })?,
        send: vec![
            Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity,
            },
        ],
    });
    Ok(HandleResponse {
        messages: vec![unlock_position_funds, increase_allowance, stake],
        log: vec![],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {}
}
