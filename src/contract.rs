use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{read_config, write_config, Config};
use crate::util::*;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        owner: deps.api.addr_canonicalize(&info.sender.to_string())?,
        anchor_ust_cw20_addr: deps.api.addr_canonicalize(&msg.anchor_ust_cw20_addr)?,
        mirror_collateral_oracle_addr: deps
            .api
            .addr_canonicalize(&msg.mirror_collateral_oracle_addr)?,
        mirror_lock_addr: deps.api.addr_canonicalize(&msg.mirror_lock_addr)?,
        mirror_mint_addr: deps.api.addr_canonicalize(&msg.mirror_mint_addr)?,
        mirror_oracle_addr: deps.api.addr_canonicalize(&msg.mirror_oracle_addr)?,
        mirror_staking_addr: deps.api.addr_canonicalize(&msg.mirror_staking_addr)?,
        spectrum_mirror_farms_addr: deps
            .api
            .addr_canonicalize(&msg.spectrum_mirror_farms_addr)?,
        spectrum_staker_addr: deps.api.addr_canonicalize(&msg.spectrum_staker_addr)?,
        terraswap_factory_addr: deps.api.addr_canonicalize(&msg.terraswap_factory_addr)?,
    };
    write_config(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let state = read_config(deps.storage)?;
    if deps.api.addr_canonicalize(&info.sender.to_string())? != state.owner {
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
            deps,
            cdp_idx,
            mirror_asset_amount,
            stake_via_spectrum,
        ),
        ExecuteMsg::CloseShortPosition { cdp_idx } => close_short_position(deps, env, cdp_idx),
        ExecuteMsg::DeltaNeutralInvest {
            collateral_asset_amount,
            collateral_ratio_in_percentage,
            mirror_asset_to_mint_cw20_addr,
        } => try_delta_neutral_invest(
            deps,
            collateral_asset_amount,
            collateral_ratio_in_percentage,
            mirror_asset_to_mint_cw20_addr,
        ),
        ExecuteMsg::Do { cosmos_messages } => try_to_do(deps, env, cosmos_messages),
    }
}

pub fn try_to_do(
    _deps: DepsMut,
    _env: Env,
    cosmos_messages: Vec<CosmosMsg>,
) -> StdResult<Response> {
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

pub fn try_delta_neutral_invest(
    deps: DepsMut,
    collateral_asset_amount: Uint128,
    collateral_ratio_in_percentage: Uint128,
    mirror_asset_to_mint_cw20_addr: String,
) -> StdResult<Response> {
    let state = read_config(deps.storage)?;
    let collateral_ratio = Decimal::from_ratio(collateral_ratio_in_percentage, 100u128);
    let inverse_collateral_ratio = Decimal::from_ratio(100u128, collateral_ratio_in_percentage);

    let collateral_price_response: mirror_protocol::collateral_oracle::CollateralPriceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps
                .api
                .addr_humanize(&state.mirror_collateral_oracle_addr)?
                .to_string(),
            msg: to_binary(
                &mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
                    asset: deps
                        .api
                        .addr_humanize(&state.anchor_ust_cw20_addr)?
                        .to_string(),
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
            contract_addr: deps
                .api
                .addr_humanize(&state.mirror_oracle_addr)?
                .to_string(),
            msg: to_binary(&mirror_protocol::oracle::QueryMsg::Price {
                base_asset: mirror_asset_to_mint_cw20_addr.to_string(),
                quote_asset: String::from("uusd"),
            })?,
        }))?;
    let minted_mirror_asset_amount: Uint128 = minted_mirror_asset_value_in_uusd
        * inverse_decimal(mirror_asset_oracle_price_response.rate);

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_to_mint_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        deps.api.addr_humanize(&state.terraswap_factory_addr)?,
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
        contract_addr: deps
            .api
            .addr_humanize(&state.anchor_ust_cw20_addr)?
            .to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: deps.api.addr_humanize(&state.mirror_mint_addr)?.to_string(),
            amount: collateral_asset_amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::OpenPosition {
                asset_info: terraswap::asset::AssetInfo::Token {
                    contract_addr: mirror_asset_to_mint_cw20_addr,
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

pub fn close_short_position(deps: DepsMut, env: Env, cdp_idx: Uint128) -> StdResult<Response> {
    let state = read_config(deps.storage)?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.addr_humanize(&state.mirror_mint_addr)?.to_string(),
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
        let mirror_asset_cw20_ask_amount = mirror_asset_cw20_amount - mirror_asset_cw20_balance;
        let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
        let terraswap_pair_info = terraswap::querier::query_pair_info(
            &deps.querier,
            deps.api.addr_humanize(&state.terraswap_factory_addr)?,
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

    let close_cdp = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_asset_cw20_addr,
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: deps.api.addr_humanize(&state.mirror_mint_addr)?.to_string(),
            amount: position_response.asset.amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: cdp_idx,
            })?,
        })?,
        funds: vec![],
    });
    response = response.add_message(close_cdp);

    Ok(response)
}

pub fn claim_short_sale_proceeds_and_stake(
    deps: DepsMut,
    cdp_idx: Uint128,
    mirror_asset_amount: Uint128,
    stake_via_spectrum: bool,
) -> StdResult<Response> {
    let state = read_config(deps.storage)?;

    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.addr_humanize(&state.mirror_mint_addr)?.to_string(),
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
    let staking_contract_addr = deps.api.addr_humanize(if stake_via_spectrum {
        &state.spectrum_staker_addr
    } else {
        &state.mirror_staking_addr
    })?;
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
        contract_addr: deps.api.addr_humanize(&state.mirror_lock_addr)?.to_string(),
        msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
            positions_idx: vec![cdp_idx],
        })?,
        funds: vec![],
    });

    let terraswap_pair_asset_info = get_terraswap_pair_asset_info(&mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        deps.api.addr_humanize(&state.terraswap_factory_addr)?,
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
            contract_addr: deps
                .api
                .addr_humanize(&state.spectrum_staker_addr)?
                .to_string(),
            msg: to_binary(&spectrum_protocol::staker::ExecuteMsg::bond {
                contract: deps
                    .api
                    .addr_humanize(&state.spectrum_mirror_farms_addr)?
                    .to_string(),
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
                staker_addr: None,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_amount_to_provide_liquidity_plus_tax_cap,
            }],
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps
                .api
                .addr_humanize(&state.mirror_staking_addr)?
                .to_string(),
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
