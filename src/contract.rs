use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier, QueryRequest, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    _msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        owner: deps.api.canonical_address(&env.message.sender)?,
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
        HandleMsg::DeltaNeutralInvest {} => try_delta_neutral_invest(deps, env),
    }
}

pub fn try_delta_neutral_invest<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
) -> StdResult<HandleResponse> {
    // All hardcoded addresses are for the testnet.
    let anchor_ust_cw20_addr = HumanAddr::from("terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl");
    let mirror_mint_addr = HumanAddr::from("terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w");
    let mirror_staking_addr = HumanAddr::from("terra1a06dgl27rhujjphsn4drl242ufws267qxypptx");
    let mirror_aapl_cw20_addr = HumanAddr::from("terra16vfxm98rxlc8erj4g0sj5932dvylgmdufnugk0");
    let terraswap_factory_addr = HumanAddr::from("terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf");

    let anchor_ust_collateral_amount = Uint128::from(1000u128);
    let collateral_ratio = Decimal::percent(200);
    let join_short_farm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: anchor_ust_cw20_addr,
        msg: to_binary(&cw20::Cw20HandleMsg::Send {
            contract: mirror_mint_addr,
            amount: anchor_ust_collateral_amount,
            msg: Some(to_binary(&mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                asset_info: terraswap::asset::AssetInfo::Token {
                    contract_addr: mirror_aapl_cw20_addr.clone(),
                },
                collateral_ratio: collateral_ratio,
                short_params: None,
            })?),
        })?,
        send: vec![],
    });

    let terraswap_pair_query_result: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: terraswap_factory_addr,
        msg: to_binary(&terraswap::factory::QueryMsg::Pair {
            asset_infos: [
                terraswap::asset::AssetInfo::Token {
                    contract_addr: mirror_aapl_cw20_addr.clone(),
                },
                terraswap::asset::AssetInfo::NativeToken {
                    denom: String::from("uusd"),
                },
            ],
        })?,
    }))?;
    let terraswap_pair_info: terraswap::asset::PairInfo = from_binary(&terraswap_pair_query_result)?;
    let terraswap_masset_ust_pair_addr = &terraswap_pair_info.contract_addr;

    let uusd_swap_amount = Uint128::from(1000u128);
    let swap_ust_for_masset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_masset_ust_pair_addr.clone(),
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
        contract_addr: mirror_aapl_cw20_addr.clone(),
        msg: to_binary(&cw20::Cw20HandleMsg::IncreaseAllowance {
            spender: mirror_staking_addr.clone(),
            amount: mirror_asset_amount,
            expires: None,
        })?,
        send: vec![],
    });

    let uusd_amount = Uint128::from(1000000000u128);
    let join_long_farm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_staking_addr,
        msg: to_binary(&mirror_protocol::staking::HandleMsg::AutoStake {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: mirror_aapl_cw20_addr,
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
