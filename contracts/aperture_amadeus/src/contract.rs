use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QueryRequest, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::*;
use crate::util::*;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        owner: info.sender,
        controller: deps.api.addr_validate(&msg.controller)?,
        anchor_ust_cw20_addr: deps.api.addr_validate(&msg.anchor_ust_cw20_addr)?,
        mirror_cw20_addr: deps.api.addr_validate(&msg.mirror_cw20_addr)?,
        spectrum_cw20_addr: deps.api.addr_validate(&msg.spectrum_cw20_addr)?,
        anchor_market_addr: deps.api.addr_validate(&msg.anchor_market_addr)?,
        mirror_collateral_oracle_addr: deps
            .api
            .addr_validate(&msg.mirror_collateral_oracle_addr)?,
        mirror_lock_addr: deps.api.addr_validate(&msg.mirror_lock_addr)?,
        mirror_mint_addr: deps.api.addr_validate(&msg.mirror_mint_addr)?,
        mirror_oracle_addr: deps.api.addr_validate(&msg.mirror_oracle_addr)?,
        mirror_staking_addr: deps.api.addr_validate(&msg.mirror_staking_addr)?,
        spectrum_gov_addr: deps.api.addr_validate(&msg.spectrum_gov_addr)?,
        spectrum_mirror_farms_addr: deps.api.addr_validate(&msg.spectrum_mirror_farms_addr)?,
        spectrum_staker_addr: deps.api.addr_validate(&msg.spectrum_staker_addr)?,
        terraswap_factory_addr: deps.api.addr_validate(&msg.terraswap_factory_addr)?,
    };
    write_config(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let is_authorized = match msg {
        ExecuteMsg::Reinvest {} => info.sender == config.owner || info.sender == config.controller,
        _ => info.sender == config.owner,
    };
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "unauthorized".to_string(),
        });
    }
    match msg {
        ExecuteMsg::ClaimShortSaleProceedsAndStake {
            cdp_idx,
            mirror_asset_amount,
            stake_via_spectrum,
        } => claim_short_sale_proceeds_and_stake(
            deps.as_ref(),
            cdp_idx,
            mirror_asset_amount,
            stake_via_spectrum,
        ),
        ExecuteMsg::CloseShortPosition { cdp_idx } => {
            close_short_position(deps.as_ref(), env, cdp_idx)
        }
        ExecuteMsg::DeltaNeutralInvest {
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        } => initiate_delta_neutral_invest(
            deps,
            env,
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        ),
        ExecuteMsg::Do { cosmos_messages } => try_to_do(cosmos_messages),
        ExecuteMsg::Reinvest {} => reinvest(deps, env),
        ExecuteMsg::SetController { controller } => set_controller(deps, controller),
    }
}

pub fn set_controller(deps: DepsMut, controller: String) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    config.controller = deps.api.addr_validate(&controller)?;
    write_config(deps.storage, &config).unwrap();
    Ok(Response::default())
}

pub fn reinvest(deps: DepsMut, env: Env) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    // Find claimable SPEC reward.
    let spec_reward_info_response: spectrum_protocol::mirror_farm::RewardInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.spectrum_mirror_farms_addr.to_string(),
            msg: to_binary(&spectrum_protocol::mirror_farm::QueryMsg::reward_info {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            })?,
        }))?;
    let mut spec_reward = Uint128::zero();
    for reward_info in spec_reward_info_response.reward_infos.iter() {
        spec_reward += reward_info.pending_farm_reward;
    }

    // Find claimable MIR reward.
    let mir_reward_info_response: mirror_protocol::staking::RewardInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::QueryMsg::RewardInfo {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            })?,
        }))?;
    let mut mir_reward = Uint128::zero();
    for reward_info in mir_reward_info_response.reward_infos.iter() {
        mir_reward += reward_info.pending_reward;
    }

    let mut response = Response::new();
    let mut reward_uusd = Uint128::zero();
    if spec_reward > Uint128::zero() {
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.spectrum_gov_addr.to_string(),
            msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {})?,
            funds: vec![],
        }));
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.spectrum_mirror_farms_addr.to_string(),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::withdraw {
                asset_token: None,
            })?,
            funds: vec![],
        }));

        let spec_uusd_terraswap_pair_addr = terraswap::querier::query_pair_info(
            &deps.querier,
            config.terraswap_factory_addr.clone(),
            &get_terraswap_pair_asset_info(&config.spectrum_cw20_addr.as_str()),
        )?
        .contract_addr;
        let simulation_response: terraswap::pair::SimulationResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: spec_uusd_terraswap_pair_addr.clone(),
                msg: to_binary(&terraswap::pair::QueryMsg::Simulation {
                    offer_asset: terraswap::asset::Asset {
                        amount: spec_reward,
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: config.spectrum_cw20_addr.to_string(),
                        },
                    },
                })?,
            }))?;
        reward_uusd += simulation_response.return_amount;

        // Swap SPEC for uusd.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.spectrum_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: spec_uusd_terraswap_pair_addr,
                amount: spec_reward,
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        }));
    }
    if mir_reward > Uint128::zero() {
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::Withdraw { asset_token: None })?,
            funds: vec![],
        }));

        let mir_uusd_terraswap_pair_addr = terraswap::querier::query_pair_info(
            &deps.querier,
            config.terraswap_factory_addr,
            &get_terraswap_pair_asset_info(&config.mirror_cw20_addr.as_str()),
        )?
        .contract_addr;
        let simulation_response: terraswap::pair::SimulationResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: mir_uusd_terraswap_pair_addr.clone(),
                msg: to_binary(&terraswap::pair::QueryMsg::Simulation {
                    offer_asset: terraswap::asset::Asset {
                        amount: mir_reward,
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: config.mirror_cw20_addr.to_string(),
                        },
                    },
                })?,
            }))?;
        reward_uusd += simulation_response.return_amount;

        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.mirror_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: mir_uusd_terraswap_pair_addr,
                amount: mir_reward,
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        }));
    }

    response = response.add_submessage(SubMsg {
        id: 1,
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: reward_uusd,
            }],
        }),
        reply_on: ReplyOn::Success,
        gas_limit: None,
    });
    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    if msg.id == 1 {
        return add_aust_to_collateral(deps.as_ref(), env);
    }
    if msg.id == 2 {
        let aust_amount = get_aust_balance(deps.as_ref(), env.contract.address)?;
        let request = read_delta_neutral_invest_request(deps.storage)?;
        return execute_delta_neutral_invest(
            deps.as_ref(),
            aust_amount,
            request.collateral_ratio_in_percentage,
            request.mirror_asset_cw20_addr,
        );
    }
    Err(StdError::GenericErr {
        msg: "unexpected_reply_id".to_string(),
    })
}

fn get_first_position_index(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = read_config(deps.storage)?;
    let positions_response: mirror_protocol::mint::PositionsResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_mint_addr.to_string(),
            msg: to_binary(&mirror_protocol::mint::QueryMsg::Positions {
                owner_addr: Some(env.contract.address.to_string()),
                asset_token: None,
                start_after: None,
                limit: None,
                order_by: None,
            })?,
        }))?;
    Ok(positions_response.positions[0].idx)
}

fn get_aust_balance(deps: Deps, address: Addr) -> StdResult<Uint128> {
    let config = read_config(deps.storage)?;
    terraswap::querier::query_token_balance(&deps.querier, config.anchor_ust_cw20_addr, address)
}

fn add_aust_to_collateral(deps: Deps, env: Env) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let aust_amount = get_aust_balance(deps, env.contract.address.clone())?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: config.mirror_mint_addr.to_string(),
                amount: aust_amount,
                msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Deposit {
                    position_idx: get_first_position_index(deps, env)?,
                })?,
            })?,
            funds: vec![],
        })),
    )
}

pub fn try_to_do(cosmos_messages: Vec<CosmosMsg>) -> StdResult<Response> {
    let mut response = Response::new();
    for message in cosmos_messages.iter() {
        response.messages.push(SubMsg {
            id: 0, // unused since reply_on is ReplyOn::Never.
            msg: message.clone(),
            reply_on: ReplyOn::Never,
            gas_limit: None,
        });
    }
    Ok(response)
}

pub fn initiate_delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    let uusd_balance = terraswap::querier::query_balance(
        &deps.querier,
        env.contract.address,
        String::from("uusd"),
    )?;
    let anchor_deposit_amount = uusd_balance.multiply_ratio(
        collateral_ratio_in_percentage,
        collateral_ratio_in_percentage.checked_add(Uint128::from(101u128))?,
    );
    let config = read_config(deps.storage)?;
    write_delta_neutral_invest_request(
        deps.storage,
        &DeltaNeutralInvestRequest {
            collateral_ratio_in_percentage,
            mirror_asset_cw20_addr,
        },
    )?;
    Ok(Response::new().add_submessage(SubMsg {
        id: 2,
        msg: CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: anchor_deposit_amount,
            }],
        }),
        reply_on: ReplyOn::Success,
        gas_limit: None,
    }))
}

pub fn execute_delta_neutral_invest(
    deps: Deps,
    collateral_asset_amount: Uint128,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let collateral_ratio = Decimal::from_ratio(collateral_ratio_in_percentage, 100u128);
    let inverse_collateral_ratio = Decimal::from_ratio(100u128, collateral_ratio_in_percentage);

    let collateral_price_response: mirror_protocol::collateral_oracle::CollateralPriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_collateral_oracle_addr.to_string(),
            msg: to_binary(
                &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
                    asset: config.anchor_ust_cw20_addr.to_string(),
                    block_height: None,
                },
            )?,
        }))?;
    let collateral_value_in_uusd: Uint128 =
        collateral_asset_amount * collateral_price_response.rate;
    let minted_mirror_asset_value_in_uusd: Uint128 =
        collateral_value_in_uusd * inverse_collateral_ratio;

    let mirror_asset_oracle_price_response: mirror_protocol::oracle::PriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_oracle_addr.to_string(),
            msg: to_binary(&mirror_protocol::oracle::QueryMsg::Price {
                base_asset: mirror_asset_cw20_addr.to_string(),
                quote_asset: String::from("uusd"),
            })?,
        }))?;
    let minted_mirror_asset_amount: Uint128 = minted_mirror_asset_value_in_uusd
        * inverse_decimal(mirror_asset_oracle_price_response.rate);

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        config.terraswap_factory_addr,
        &terraswap_pair_asset_info,
    )?;
    let uusd_swap_amount = get_uusd_amount_to_swap_for_long_position(
        &deps.querier,
        deps.api,
        &terraswap_pair_info.contract_addr,
        &terraswap_pair_asset_info[0],
        &terraswap_pair_asset_info[1],
        minted_mirror_asset_amount,
    )?;

    let open_cdp = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.anchor_ust_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: config.mirror_mint_addr.to_string(),
            amount: collateral_asset_amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                asset_info: terraswap::asset::AssetInfo::Token {
                    contract_addr: mirror_asset_cw20_addr,
                },
                collateral_ratio,
                short_params: Some(mirror_protocol::mint::ShortParams {
                    belief_price: None,
                    max_spread: None,
                }),
            })?,
        })?,
        funds: vec![],
    });

    let swap_uusd_for_mirror_asset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair_info.contract_addr,
        msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
            offer_asset: terraswap::asset::Asset {
                info: terraswap_pair_asset_info[1].clone(),
                amount: uusd_swap_amount,
            },
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        funds: vec![Coin {
            denom: String::from("uusd"),
            amount: uusd_swap_amount,
        }],
    });

    Ok(Response::new()
        .add_message(open_cdp)
        .add_message(swap_uusd_for_mirror_asset))
}

pub fn close_short_position(deps: Deps, env: Env, cdp_idx: Uint128) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_mint_addr.to_string(),
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
        &deps.querier,
        deps.api.addr_validate(&mirror_asset_cw20_addr)?,
        env.contract.address,
    )?;

    let mut response = Response::new();
    if mirror_asset_cw20_balance < mirror_asset_cw20_amount {
        let mirror_asset_cw20_ask_amount =
            mirror_asset_cw20_amount.checked_sub(mirror_asset_cw20_balance)?;
        let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
        let terraswap_pair_info = terraswap::querier::query_pair_info(
            &deps.querier,
            config.terraswap_factory_addr,
            &terraswap_pair_asset_info,
        )?;
        let reverse_simulation_response: terraswap::pair::ReverseSimulationResponse =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: terraswap_pair_info.contract_addr.clone(),
                msg: to_binary(&terraswap::pair::QueryMsg::ReverseSimulation {
                    ask_asset: terraswap::asset::Asset {
                        amount: mirror_asset_cw20_ask_amount,
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: mirror_asset_cw20_addr.clone(),
                        },
                    },
                })?,
            }))?;
        let swap_uusd_for_mirror_asset = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair_info.contract_addr,
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
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
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: reverse_simulation_response.offer_amount,
            }],
        });
        response = response.add_message(swap_uusd_for_mirror_asset);
    }

    let burn_minted_mirror_asset = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.clone(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: config.mirror_mint_addr.to_string(),
            amount: position_response.asset.amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: cdp_idx,
            })?,
        })?,
        funds: vec![],
    });
    response = response.add_message(burn_minted_mirror_asset);

    let withdraw_collateral = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr,
        msg: to_binary(&mirror_protocol::mint::ExecuteMsg::Withdraw {
            collateral: None,
            position_idx: cdp_idx,
        })?,
        funds: vec![],
    });
    response = response.add_message(withdraw_collateral);

    Ok(response)
}

pub fn claim_short_sale_proceeds_and_stake(
    deps: Deps,
    cdp_idx: Uint128,
    mirror_asset_amount: Uint128,
    stake_via_spectrum: bool,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.mirror_mint_addr.to_string(),
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
    let staking_contract_addr = if stake_via_spectrum {
        config.spectrum_staker_addr.clone()
    } else {
        config.mirror_staking_addr.clone()
    };
    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr.clone(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: staking_contract_addr.to_string(),
            amount: mirror_asset_amount,
            expires: None,
        })?,
        funds: vec![],
    });

    let unlock_position_funds = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.mirror_lock_addr.to_string(),
        msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
            positions_idx: vec![cdp_idx],
        })?,
        funds: vec![],
    });

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        config.terraswap_factory_addr,
        &terraswap_pair_asset_info,
    )?;
    let pool_mirror_asset_balance = terraswap_pair_asset_info[0].query_pool(
        &deps.querier,
        deps.api,
        deps.api.addr_validate(&terraswap_pair_info.contract_addr)?,
    )?;
    let pool_uusd_balance = terraswap_pair_asset_info[1].query_pool(
        &deps.querier,
        deps.api,
        deps.api.addr_validate(&terraswap_pair_info.contract_addr)?,
    )?;
    let uusd_amount_to_provide_liquidity =
        mirror_asset_amount.multiply_ratio(pool_uusd_balance, pool_mirror_asset_balance);
    let uusd_amount_to_provide_liquidity_plus_tax_cap =
        uusd_amount_to_provide_liquidity + get_tax_cap_in_uusd(&deps.querier)?;

    let stake = if stake_via_spectrum {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.spectrum_staker_addr.to_string(),
            msg: to_binary(&spectrum_protocol::staker::ExecuteMsg::bond {
                contract: config.spectrum_mirror_farms_addr.to_string(),
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
                slippage_tolerance: Decimal::percent(50u64),
                compound_rate: Some(Decimal::one()),
                staker_addr: None,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity_plus_tax_cap,
            }],
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::AutoStake {
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
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity,
            }],
        })
    };
    Ok(Response::new()
        .add_message(unlock_position_funds)
        .add_message(increase_allowance)
        .add_message(stake))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}