use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, InitResponse, Querier, QueryRequest, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::math::{decimal_division, decimal_multiplication};
use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};

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
        HandleMsg::DeltaNeutralInvest {collateral_asset_amount, collateral_ratio} =>
            try_delta_neutral_invest(deps, env, collateral_asset_amount, collateral_ratio),
        HandleMsg::Do {cosmos_messages} => try_to_do(deps, env, cosmos_messages),
        HandleMsg::Receive {cw20_receive_msg} => receive_cw20(deps, env, cw20_receive_msg),
    }
}

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _cw20_receive_msg: cw20::Cw20ReceiveMsg,
) -> StdResult<HandleResponse> {
    // TODO: Implement a couple of hook messages for delta_neutral and deposit.
    Ok(HandleResponse::default())
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
    collateral_ratio: Decimal,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let collateral_price_query_result: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(&state.mirror_collateral_oracle_addr)?,
        msg: to_binary(&mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
            asset: deps.api.human_address(&state.anchor_ust_cw20_addr)?.to_string(),
        })?,
    }))?;
    let collateral_price_response: mirror_protocol::collateral_oracle::CollateralPriceResponse =
        from_binary(&collateral_price_query_result)?;
    let collateral_ust_value: Decimal = decimal_multiplication(Decimal::from_ratio(collateral_asset_amount, 1u128), collateral_price_response.rate);
    let ust_value_to_mint_masset: Decimal = decimal_division(collateral_ust_value, collateral_ratio);

    let mirror_asset_oracle_price_result: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(&state.mirror_oracle_addr)?,
        msg: to_binary(&mirror_protocol::oracle::QueryMsg::Price {
            base_asset: deps.api.human_address(&state.mirror_asset_cw20_addr)?.to_string(),
            quote_asset: String::from("uusd"),
        })?,
    }))?;
    let mirror_asset_oracle_price_response: mirror_protocol::oracle::PriceResponse =
        from_binary(&mirror_asset_oracle_price_result)?;
    let mirror_asset_oracle_price_in_uusd: Decimal = mirror_asset_oracle_price_response.rate;

    let join_short_farm = CosmosMsg::Wasm(WasmMsg::Execute {
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

    let terraswap_pair_query_result: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(&state.terraswap_factory_addr)?,
        msg: to_binary(&terraswap::factory::QueryMsg::Pair {
            asset_infos: [
                terraswap::asset::AssetInfo::Token {
                    contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
                },
                terraswap::asset::AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
            ],
        })?,
    }))?;
    let terraswap_pair_info: terraswap::asset::PairInfo = from_binary(&terraswap_pair_query_result)?;

    // TODO: Query Mirror Oracle for mAsset price and calculate this.
    let minted_mirror_asset_amount: Uint128 = Uint128::from(1000u128);
    let terraswap_simulation_result: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: terraswap_pair_info.contract_addr.clone(),
        msg: to_binary(&terraswap::pair::QueryMsg::Simulation {
            offer_asset: terraswap::asset::Asset {
                info: terraswap::asset::AssetInfo::Token {
                    contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
                },
                amount: minted_mirror_asset_amount,
            },
        })?,
    }))?;

    let uusd_swap_amount = Uint128::from(1000u128);
    let swap_ust_for_masset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair_info.contract_addr,
        msg: to_binary(&terraswap::pair::HandleMsg::Swap {
            offer_asset: terraswap::asset::Asset {
                info: terraswap::asset::AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
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

    let mirror_asset_amount = Uint128::from(5u128);
    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.mirror_asset_cw20_addr)?,
        msg: to_binary(&cw20::Cw20HandleMsg::IncreaseAllowance {
            spender: deps.api.human_address(&state.mirror_staking_addr)?,
            amount: mirror_asset_amount,
            expires: None,
        })?,
        send: vec![],
    });

    let uusd_amount = Uint128::from(1000000000u128);
    let join_long_farm = CosmosMsg::Wasm(WasmMsg::Execute {
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
                    amount: uusd_amount,
                },
            ],
            slippage_tolerance: None,
        })?,
        send: vec![
            Coin {
                denom: String::from("uusd"),
                amount: uusd_amount,
            },
        ],
    });

    let response = HandleResponse {
        messages: vec![
            join_short_farm,
            swap_ust_for_masset,
            increase_allowance,
            join_long_farm,
        ],
        log: vec![],
        data: None,
    };
    Ok(response)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
    }
}
