use cosmwasm_std::{
    to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier, StdError,
    StdResult, Storage, Uint128, WasmMsg,
};

use crate::msg::{CountResponse, HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        count: msg.count,
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
    match msg {
        HandleMsg::Increment {} => try_increment(deps, env),
        HandleMsg::Reset { count } => try_reset(deps, env, count),
        HandleMsg::DeltaNeutralInvest {} => try_delta_neutral_invest(deps, env),
    }
}

pub fn try_increment<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
) -> StdResult<HandleResponse> {
    config(&mut deps.storage).update(|mut state| {
        state.count += 1;
        Ok(state)
    })?;

    Ok(HandleResponse::default())
}

pub fn try_reset<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    count: i32,
) -> StdResult<HandleResponse> {
    let api = &deps.api;
    config(&mut deps.storage).update(|mut state| {
        if api.canonical_address(&env.message.sender)? != state.owner {
            return Err(StdError::unauthorized());
        }
        state.count = count;
        Ok(state)
    })?;
    Ok(HandleResponse::default())
}

pub fn try_delta_neutral_invest<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
) -> StdResult<HandleResponse> {
    // All hardcoded addresses are for the testnet.
    let anchor_ust_cw20_addr = HumanAddr::from("terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl");
    let mirror_mint_addr = HumanAddr::from("terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w");
    let mirror_staking_addr = HumanAddr::from("terra1a06dgl27rhujjphsn4drl242ufws267qxypptx");
    let mirror_aapl_cw20_addr = HumanAddr::from("terra16vfxm98rxlc8erj4g0sj5932dvylgmdufnugk0");

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

    let uusd_swap_amount = Uint128::from(1000u128);
    // TODO: Need to first query Terraswap factory for pair address.
    let terraswap_masset_ust_pair_addr = HumanAddr::from("TODO");
    let swap_ust_for_masset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_masset_ust_pair_addr,
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
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => to_binary(&query_count(deps)?),
    }
}

fn query_count<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<CountResponse> {
    let state = config_read(&deps.storage).load()?;
    Ok(CountResponse { count: state.count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(20, &[]);

        let msg = InitMsg { count: 17 };
        let env = mock_env("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(&deps, QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(17, value.count);
    }

    #[test]
    fn increment() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let msg = InitMsg { count: 17 };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        // beneficiary can release it
        let env = mock_env("anyone", &coins(2, "token"));
        let msg = HandleMsg::Increment {};
        let _res = handle(&mut deps, env, msg).unwrap();

        // should increase counter by 1
        let res = query(&deps, QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(18, value.count);
    }

    #[test]
    fn reset() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let msg = InitMsg { count: 17 };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        // beneficiary can release it
        let unauth_env = mock_env("anyone", &coins(2, "token"));
        let msg = HandleMsg::Reset { count: 5 };
        let res = handle(&mut deps, unauth_env, msg);
        match res {
            Err(StdError::Unauthorized { .. }) => {}
            _ => panic!("Must return unauthorized error"),
        }

        // only the original creator can reset the counter
        let auth_env = mock_env("creator", &coins(2, "token"));
        let msg = HandleMsg::Reset { count: 5 };
        let _res = handle(&mut deps, auth_env, msg).unwrap();

        // should now be 5
        let res = query(&deps, QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(5, value.count);
    }
}
