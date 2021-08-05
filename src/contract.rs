use cosmwasm_std::{
    to_binary, Addr, Api, Binary, CosmosMsg, Decimal, Deps, Env, Querier, Response, StdError,
    StdResult, Storage, Uint128, WasmMsg,
};

use crate::msg::{CountResponse, HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Deps<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<Response> {
    let state = State {
        count: msg.count,
        owner: deps.api.canonical_address(&env.message.sender)?,
        // Hardcoding testnet mAAPL's address for now.
        mirror_asset_addr: deps.api.addr_validate("terra16vfxm98rxlc8erj4g0sj5932dvylgmdufnugk0")?,
        anchor_ust_addr: deps.api.addr_validate("terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl")?,
    };

    config(&mut deps.storage).save(&state)?;

    Ok(Response::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Deps<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<Response> {
    match msg {
        HandleMsg::Increment {} => try_increment(deps, env),
        HandleMsg::Reset { count } => try_reset(deps, env, count),
        HandleMsg::DeltaNeutralInvest {} => try_delta_neutral_invest(deps, env),
    }
}

pub fn try_delta_neutral_invest<S: Storage, A: Api, Q: Querier>(
    deps: &mut Deps<S, A, Q>,
    _env: Env,
) -> StdResult<Response> {
    let state = config_read(&deps.storage).load()?;
    let amount = Uint128::from(1000u128);
    let collateral_ratio = Decimal::percent(200);

    let join_short_farm = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: state.anchor_ust_addr,
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            // Mirror Mint contract address on tequlia-0004.
            contract: String::from("terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w"),
            amount: amount,
            msg: to_binary(&mirror_protocol::mint::HandleMsg::OpenPosition {
                collateral: terraswap::asset::Asset {
                    amount: amount,
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: state.anchor_ust_addr,
                    },
                },
                asset_info: terraswap::asset::AssetInfo::Token {
                    contract_addr: state.mirror_asset_addr,
                },
                collateral_ratio: collateral_ratio,
            })?,
        })?,
        send: vec![],
    });

    let response = Response {
        messages: vec![
            join_short_farm,
        ],
        log: vec![],
        data: None,
    };
    Ok(response)
}

pub fn try_increment<S: Storage, A: Api, Q: Querier>(
    deps: &mut Deps<S, A, Q>,
    _env: Env,
) -> StdResult<Response> {
    config(&mut deps.storage).update(|mut state| {
        state.count += 1;
        Ok(state)
    })?;

    Ok(Response::default())
}

pub fn try_reset<S: Storage, A: Api, Q: Querier>(
    deps: &mut Deps<S, A, Q>,
    env: Env,
    count: i32,
) -> StdResult<Response> {
    let api = &deps.api;
    config(&mut deps.storage).update(|mut state| {
        if api.canonical_address(&env.message.sender)? != state.owner {
            return Err(StdError::unauthorized());
        }
        state.count = count;
        Ok(state)
    })?;
    Ok(Response::default())
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Deps<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => to_binary(&query_count(deps)?),
    }
}

fn query_count<S: Storage, A: Api, Q: Querier>(deps: &Deps<S, A, Q>) -> StdResult<CountResponse> {
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
