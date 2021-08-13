use cosmwasm_std::{
    to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, QueryRequest, StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::state::{config, config_read, State};
use crate::util::*;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        owner: deps.api.canonical_address(&env.message.sender)?,
        anchor_ust_cw20_addr: deps.api.canonical_address(&msg.anchor_ust_cw20_addr)?,
        mirror_collateral_oracle_addr: deps
            .api
            .canonical_address(&msg.mirror_collateral_oracle_addr)?,
        mirror_lock_addr: deps.api.canonical_address(&msg.mirror_lock_addr)?,
        mirror_mint_addr: deps.api.canonical_address(&msg.mirror_mint_addr)?,
        mirror_oracle_addr: deps.api.canonical_address(&msg.mirror_oracle_addr)?,
        mirror_staking_addr: deps.api.canonical_address(&msg.mirror_staking_addr)?,
        spectrum_mirror_farms_addr: deps
            .api
            .canonical_address(&msg.spectrum_mirror_farms_addr)?,
        spectrum_staker_addr: deps.api.canonical_address(&msg.spectrum_staker_addr)?,
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
        HandleMsg::ClaimShortSaleProceedsAndStake {
            cdp_idx,
            mirror_asset_amount,
            stake_via_spectrum,
        } => claim_short_sale_proceeds_and_stake(
            deps,
            cdp_idx,
            mirror_asset_amount,
            stake_via_spectrum,
        ),
        HandleMsg::CloseShortPosition { cdp_idx } => close_short_position(deps, env, cdp_idx),
        HandleMsg::DeltaNeutralInvest {
            collateral_asset_amount,
            collateral_ratio_in_percentage,
            mirror_asset_to_mint_cw20_addr,
        } => try_delta_neutral_invest(
            deps,
            collateral_asset_amount,
            collateral_ratio_in_percentage,
            mirror_asset_to_mint_cw20_addr,
        ),
        HandleMsg::Do { cosmos_messages } => try_to_do(deps, env, cosmos_messages),
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
    collateral_asset_amount: Uint128,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_to_mint_cw20_addr: HumanAddr,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;
    let collateral_ratio = Decimal::from_ratio(collateral_ratio_in_percentage, 100u128);
    let inverse_collateral_ratio = Decimal::from_ratio(100u128, collateral_ratio_in_percentage);

    let collateral_price_response: mirror_protocol::collateral_oracle::CollateralPriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps
                .api
                .human_address(&state.mirror_collateral_oracle_addr)?,
            msg: to_binary(
                &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
                    asset: deps
                        .api
                        .human_address(&state.anchor_ust_cw20_addr)?
                        .to_string(),
                },
            )?,
        }))?;
    let collateral_value_in_uusd: Uint128 =
        collateral_asset_amount * collateral_price_response.rate;
    let minted_mirror_asset_value_in_uusd: Uint128 =
        collateral_value_in_uusd * inverse_collateral_ratio;

    let mirror_asset_oracle_price_response: mirror_protocol::oracle::PriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&state.mirror_oracle_addr)?,
            msg: to_binary(&mirror_protocol::oracle::QueryMsg::Price {
                base_asset: mirror_asset_to_mint_cw20_addr.to_string(),
                quote_asset: String::from("uusd"),
            })?,
        }))?;
    let minted_mirror_asset_amount: Uint128 = minted_mirror_asset_value_in_uusd
        * inverse_decimal(mirror_asset_oracle_price_response.rate);

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_to_mint_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        deps,
        &deps.api.human_address(&state.terraswap_factory_addr)?,
        &terraswap_pair_asset_info,
    )?;
    let uusd_swap_amount = get_uusd_amount_to_swap_for_long_position(
        deps,
        &terraswap_pair_info.contract_addr,
        &terraswap_pair_asset_info[0],
        &terraswap_pair_asset_info[1],
        minted_mirror_asset_amount,
    )?;

    let open_cdp = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.anchor_ust_cw20_addr)?,
        msg: to_binary(&cw20::Cw20HandleMsg::Send {
            contract: deps.api.human_address(&state.mirror_mint_addr)?,
            amount: collateral_asset_amount,
            msg: Some(to_binary(
                &mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                    asset_info: terraswap::asset::AssetInfo::Token {
                        contract_addr: mirror_asset_to_mint_cw20_addr,
                    },
                    collateral_ratio,
                    short_params: Some(mirror_protocol::mint::ShortParams {
                        belief_price: None,
                        max_spread: None,
                    }),
                },
            )?),
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
        send: vec![Coin {
            denom: String::from("uusd"),
            amount: uusd_swap_amount,
        }],
    });

    let response = HandleResponse {
        messages: vec![open_cdp, swap_uusd_for_mirror_asset],
        log: vec![],
        data: None,
    };
    Ok(response)
}

pub fn close_short_position<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    cdp_idx: Uint128,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&state.mirror_mint_addr)?,
            msg: to_binary(&mirror_protocol::mint::QueryMsg::Position {
                position_idx: cdp_idx,
            })?,
        }))?;
    let mirror_asset_cw20_addr = if let terraswap::asset::AssetInfo::Token {
        contract_addr: addr,
    } = position_response.asset.info
    {
        addr
    } else {
        unreachable!()
    };
    let mirror_asset_cw20_amount = position_response.asset.amount;
    let mirror_asset_cw20_balance = terraswap::querier::query_token_balance(
        deps,
        &mirror_asset_cw20_addr,
        &env.contract.address,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if mirror_asset_cw20_balance < mirror_asset_cw20_amount {
        let mirror_asset_cw20_ask_amount = (mirror_asset_cw20_amount - mirror_asset_cw20_balance)?;
        let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
        let terraswap_pair_info = terraswap::querier::query_pair_info(
            deps,
            &deps.api.human_address(&state.terraswap_factory_addr)?,
            &terraswap_pair_asset_info,
        )?;
        let reverse_simulation_response: terraswap::pair::ReverseSimulationResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: terraswap_pair_info.contract_addr.clone(),
                msg: to_binary(&terraswap::pair::QueryMsg::ReverseSimulation {
                    ask_asset: terraswap::asset::Asset {
                        amount: mirror_asset_cw20_ask_amount,
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr,
                        },
                    },
                })?,
            }))?;
        let swap_uusd_for_mirror_asset = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair_info.contract_addr,
            msg: to_binary(&terraswap::pair::HandleMsg::Swap {
                offer_asset: terraswap::asset::Asset {
                    amount: reverse_simulation_response.offer_amount,
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                },
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
            send: vec![Coin {
                denom: String::from("uusd"),
                amount: reverse_simulation_response.offer_amount,
            }],
        });
        messages.push(swap_uusd_for_mirror_asset);
    }

    let close_cdp = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&state.anchor_ust_cw20_addr)?,
        msg: to_binary(&cw20::Cw20HandleMsg::Send {
            contract: deps.api.human_address(&state.mirror_mint_addr)?,
            amount: position_response.asset.amount,
            msg: Some(to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: cdp_idx,
            })?),
        })?,
        send: vec![],
    });
    messages.push(close_cdp);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

pub fn claim_short_sale_proceeds_and_stake<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    cdp_idx: Uint128,
    mirror_asset_amount: Uint128,
    stake_via_spectrum: bool,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&state.mirror_mint_addr)?,
            msg: to_binary(&mirror_protocol::mint::QueryMsg::Position {
                position_idx: cdp_idx,
            })?,
        }))?;
    let mirror_asset_cw20_addr = if let terraswap::asset::AssetInfo::Token {
        contract_addr: addr,
    } = position_response.asset.info
    {
        addr
    } else {
        unreachable!()
    };
    let staking_contract_addr = deps.api.human_address(if stake_via_spectrum {
        &state.spectrum_staker_addr
    } else {
        &state.mirror_staking_addr
    })?;
    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.clone(),
        msg: to_binary(&cw20::Cw20HandleMsg::IncreaseAllowance {
            spender: staking_contract_addr,
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

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        deps,
        &deps.api.human_address(&state.terraswap_factory_addr)?,
        &terraswap_pair_asset_info,
    )?;
    let pool_mirror_asset_balance =
        terraswap_pair_asset_info[0].query_pool(deps, &terraswap_pair_info.contract_addr)?;
    let pool_uusd_balance =
        terraswap_pair_asset_info[1].query_pool(deps, &terraswap_pair_info.contract_addr)?;
    let uusd_amount_to_provide_liquidity =
        mirror_asset_amount.multiply_ratio(pool_uusd_balance, pool_mirror_asset_balance);
    let uusd_amount_to_provide_liquidity_plus_tax_cap =
        uusd_amount_to_provide_liquidity + get_tax_cap_in_uusd(deps)?;

    let stake = if stake_via_spectrum {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&state.spectrum_staker_addr)?,
            msg: to_binary(&spectrum_protocol::staker::HandleMsg::bond {
                contract: deps.api.human_address(&state.spectrum_mirror_farms_addr)?,
                assets: [
                    terraswap::asset::Asset {
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr,
                        },
                        amount: mirror_asset_amount,
                    },
                    terraswap::asset::Asset {
                        info: terraswap::asset::AssetInfo::NativeToken {
                            denom: String::from("uusd"),
                        },
                        amount: uusd_amount_to_provide_liquidity_plus_tax_cap,
                    },
                ],
                slippage_tolerance: None,
                compound_rate: Some(Decimal::one()),
            })?,
            send: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity_plus_tax_cap,
            }],
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&state.mirror_staking_addr)?,
            msg: to_binary(&mirror_protocol::staking::HandleMsg::AutoStake {
                assets: [
                    terraswap::asset::Asset {
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr,
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
            send: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity,
            }],
        })
    };
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
