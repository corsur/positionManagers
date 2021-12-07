use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::state::{PositionInfo, MANAGER, POSITION_INFO};
use crate::util::{
    create_terraswap_cw20_uusd_pair_asset_info, find_collateral_uusd_amount,
    swap_cw20_token_for_uusd,
};
use aperture_common::delta_neutral_position::{
    ControllerExecuteMsg, ExecuteMsg, InstantiateMsg, InternalExecuteMsg, MigrateMsg, QueryMsg,
};
use aperture_common::delta_neutral_position_manager::QueryMsg as ManagerQueryMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    MANAGER.save(deps.storage, &info.sender)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    let manager_addr = MANAGER.load(deps.storage)?;
    let context: Context = deps
        .querier
        .query_wasm_smart(&manager_addr, &ManagerQueryMsg::GetContext {})?;
    let is_authorized = match msg {
        ExecuteMsg::Controller(_) => {
            info.sender == context.controller || info.sender == env.contract.address
        }
        ExecuteMsg::Internal(_) => info.sender == env.contract.address,
        _ => info.sender == manager_addr,
    };
    if !is_authorized {
        return Err(StdError::GenericErr {
            msg: "unauthorized".to_string(),
        });
    }
    match msg {
        ExecuteMsg::OpenPosition { params } => delta_neutral_invest(
            deps,
            env,
            context,
            params.target_min_collateral_ratio,
            params.target_max_collateral_ratio,
            params.mirror_asset_cw20_addr,
        ),
        ExecuteMsg::IncreasePosition {} => rebalance(deps.as_ref(), env, context),
        ExecuteMsg::DecreasePosition {
            proportion,
            recipient,
        } => decrease_position(deps.as_ref(), env, context, proportion, recipient),
        ExecuteMsg::Controller(controller_msg) => match controller_msg {
            ControllerExecuteMsg::Rebalance {} => rebalance(deps.as_ref(), env, context),
        },
        ExecuteMsg::Internal(internal_msg) => match internal_msg {
            InternalExecuteMsg::ClaimAndIncreaseUusdBalance {} => {
                claim_and_increase_uusd_balance(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::DepositUusdBalanceToAnchor {} => {
                deposit_uusd_balance_to_anchor(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::AddAnchorUstBalanceToCollateral {} => {
                add_anchor_ust_balance_to_collateral(deps.as_ref(), env, context)
            }
            InternalExecuteMsg::OpenCdpWithAnchorUstBalanceAsCollateral {
                collateral_ratio,
                mirror_asset_cw20_addr,
            } => open_cdp_with_anchor_ust_balance_as_collateral(
                deps.as_ref(),
                env,
                context,
                collateral_ratio,
                mirror_asset_cw20_addr,
            ),
            InternalExecuteMsg::SwapUusdForMintedMirrorAsset {} => {
                swap_uusd_for_minted_mirror_asset(deps, env, context)
            }
            InternalExecuteMsg::StakeTerraswapLpTokens {
                lp_token_cw20_addr,
                stake_via_spectrum,
            } => stake_terraswap_lp_tokens(
                deps.as_ref(),
                env,
                context,
                lp_token_cw20_addr,
                stake_via_spectrum,
            ),
        },
    }
}

pub fn deposit_uusd_balance_to_anchor(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let uusd_asset = terraswap::asset::Asset {
        amount: terraswap::querier::query_balance(
            &deps.querier,
            env.contract.address,
            String::from("uusd"),
        )?,
        info: terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    };
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![uusd_asset.deduct_tax(&deps.querier)?],
        })),
    )
}

pub fn add_anchor_ust_balance_to_collateral(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    let aust_amount = terraswap::querier::query_token_balance(
        &deps.querier,
        context.anchor_ust_cw20_addr.clone(),
        env.contract.address,
    )?;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: aust_amount,
                msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Deposit {
                    position_idx: POSITION_INFO.load(deps.storage)?.cdp_idx,
                })?,
            })?,
            funds: vec![],
        })),
    )
}

fn create_internal_execute_message(env: &Env, msg: InternalExecuteMsg) -> CosmosMsg {
    CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::Internal(msg)).unwrap(),
        funds: vec![],
    })
}

pub fn rebalance(_deps: Deps, env: Env, _context: Context) -> StdResult<Response> {
    let mut response = Response::new();
    response = response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::ClaimAndIncreaseUusdBalance {},
    ));
    // TODO: bring mAsset back to delta-neutral.
    // TODO: bring collateral ratio to target range.
    Ok(response)
}

pub fn claim_and_increase_uusd_balance(
    deps: Deps,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    // Find claimable SPEC reward.
    let spec_reward_info_response: spectrum_protocol::mirror_farm::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            &context.spectrum_mirror_farms_addr,
            &spectrum_protocol::mirror_farm::QueryMsg::reward_info {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            },
        )?;
    let mut spec_reward = Uint128::zero();
    for reward_info in spec_reward_info_response.reward_infos.iter() {
        spec_reward += reward_info.pending_spec_reward;
    }

    // Find claimable MIR reward.
    let mir_reward_info_response: mirror_protocol::staking::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_staking_addr,
            &mirror_protocol::staking::QueryMsg::RewardInfo {
                staker_addr: env.contract.address.to_string(),
                asset_token: None,
            },
        )?;
    let mut mir_reward = Uint128::zero();
    for reward_info in mir_reward_info_response.reward_infos.iter() {
        mir_reward += reward_info.pending_reward;
    }

    // Claim MIR / SPEC reward and swap them for uusd.
    let mut response = Response::new();
    if spec_reward > Uint128::zero() {
        // Mint SPEC tokens to ensure that emissable SPEC tokens are available for withdrawal.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_gov_addr.to_string(),
            msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {})?,
            funds: vec![],
        }));

        // Claim SPEC reward.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_mirror_farms_addr.to_string(),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::withdraw {
                asset_token: None,
                farm_amount: None,
                spec_amount: None,
            })?,
            funds: vec![],
        }));

        // Swap SPEC reward for uusd.
        response = response.add_message(swap_cw20_token_for_uusd(
            &deps.querier,
            context.terraswap_factory_addr.clone(),
            context.spectrum_cw20_addr.as_str(),
            spec_reward,
        )?);
    }
    if mir_reward > Uint128::zero() {
        // Claim MIR reward.
        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::Withdraw { asset_token: None })?,
            funds: vec![],
        }));

        // Swap MIR for uusd.
        response = response.add_message(swap_cw20_token_for_uusd(
            &deps.querier,
            context.terraswap_factory_addr,
            context.mirror_cw20_addr.as_str(),
            mir_reward,
        )?);
    }

    // If there are any unlocked funds in the short farm, claim them.
    let position_info = POSITION_INFO.load(deps.storage)?;
    let position_lock_info_result: StdResult<mirror_protocol::lock::PositionLockInfoResponse> =
        deps.querier.query_wasm_smart(
            &context.mirror_lock_addr,
            &mirror_protocol::lock::QueryMsg::PositionLockInfo {
                position_idx: position_info.cdp_idx,
            },
        );
    if let Ok(position_lock_info_response) = position_lock_info_result {
        if position_lock_info_response.unlock_time <= env.block.time.seconds() {
            response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.mirror_lock_addr.to_string(),
                msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
                    positions_idx: vec![position_info.cdp_idx],
                })?,
                funds: vec![],
            }));
        }
    }

    Ok(response)
}

fn get_cdp_index(deps: Deps, env: Env, context: &Context) -> StdResult<Uint128> {
    let positions_response: mirror_protocol::mint::PositionsResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Positions {
                owner_addr: Some(env.contract.address.to_string()),
                asset_token: None,
                start_after: None,
                limit: None,
                order_by: None,
            },
        )?;
    Ok(positions_response.positions[0].idx)
}

pub fn delta_neutral_invest(
    deps: DepsMut,
    env: Env,
    context: Context,
    target_min_collateral_ratio: Decimal,
    target_max_collateral_ratio: Decimal,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    if POSITION_INFO.load(deps.storage).is_ok() {
        return Err(StdError::GenericErr {
            msg: "delta_neutral_position_already_exists".to_string(),
        });
    }

    let uusd_balance = terraswap::querier::query_balance(
        &deps.querier,
        env.contract.address.clone(),
        String::from("uusd"),
    )?;
    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_market_addr.to_string(),
            msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: find_collateral_uusd_amount(
                    deps.as_ref(),
                    &context,
                    &mirror_asset_cw20_addr,
                    target_min_collateral_ratio,
                    target_max_collateral_ratio,
                    uusd_balance,
                )?,
            }],
        }))
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::OpenCdpWithAnchorUstBalanceAsCollateral {
                collateral_ratio: (target_min_collateral_ratio + target_max_collateral_ratio)
                    / 2u128.into(),
                mirror_asset_cw20_addr,
            },
        ))
        .add_message(create_internal_execute_message(
            &env,
            InternalExecuteMsg::SwapUusdForMintedMirrorAsset {},
        )))
}

fn open_cdp_with_anchor_ust_balance_as_collateral(
    deps: Deps,
    env: Env,
    context: Context,
    collateral_ratio: Decimal,
    mirror_asset_cw20_addr: String,
) -> StdResult<Response> {
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.anchor_ust_cw20_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: context.mirror_mint_addr.to_string(),
                amount: terraswap::querier::query_token_balance(
                    &deps.querier,
                    context.anchor_ust_cw20_addr,
                    env.contract.address,
                )?,
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
        })),
    )
}

fn swap_uusd_for_minted_mirror_asset(
    deps: DepsMut,
    env: Env,
    context: Context,
) -> StdResult<Response> {
    // Query position info.
    let cdp_idx = get_cdp_index(deps.as_ref(), env, &context)?;
    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Position {
                position_idx: cdp_idx,
            },
        )?;
    let mirror_asset_cw20_addr = if let terraswap::asset::AssetInfo::Token {
        contract_addr: addr,
    } = position_response.asset.info
    {
        addr
    } else {
        unreachable!()
    };

    // Write position info to storage.
    let position_info = PositionInfo {
        cdp_idx,
        mirror_asset_cw20_addr: deps.api.addr_validate(&mirror_asset_cw20_addr)?,
    };
    POSITION_INFO.save(deps.storage, &position_info)?;

    // Swap uusd for mAsset.
    let mirror_asset_uusd_terraswap_pair_addr = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr,
        &create_terraswap_cw20_uusd_pair_asset_info(&mirror_asset_cw20_addr),
    )?
    .contract_addr;
    let reverse_simulation_response: terraswap::pair::ReverseSimulationResponse =
        deps.querier.query_wasm_smart(
            &mirror_asset_uusd_terraswap_pair_addr,
            &terraswap::pair::QueryMsg::ReverseSimulation {
                ask_asset: terraswap::asset::Asset {
                    amount: position_response.asset.amount,
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: mirror_asset_cw20_addr,
                    },
                },
            },
        )?;
    let uusd_offer_amount: Uint128 = reverse_simulation_response.offer_amount;
    Ok(
        Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_asset_uusd_terraswap_pair_addr,
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                    amount: uusd_offer_amount,
                },
                max_spread: None,
                belief_price: None,
                to: None,
            })?,
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: uusd_offer_amount,
            }],
        })),
    )
}

pub fn decrease_position(
    deps: Deps,
    env: Env,
    context: Context,
    _fraction: Decimal,
    _recipient: String,
) -> StdResult<Response> {
    // TODO: Rebalance, reduce short / long / collateral positions by `fraction`, and then return UST to `recipient`.
    let position_info = POSITION_INFO.load(deps.storage)?;
    let position_response: mirror_protocol::mint::PositionResponse =
        deps.querier.query_wasm_smart(
            &context.mirror_mint_addr,
            &mirror_protocol::mint::QueryMsg::Position {
                position_idx: position_info.cdp_idx,
            },
        )?;
    let mirror_asset_cw20_amount = position_response.asset.amount;
    let mirror_asset_cw20_balance = terraswap::querier::query_token_balance(
        &deps.querier,
        deps.api
            .addr_validate(position_info.mirror_asset_cw20_addr.as_str())?,
        env.contract.address,
    )?;

    let mut response = Response::new();
    if mirror_asset_cw20_balance < mirror_asset_cw20_amount {
        let mirror_asset_cw20_ask_amount =
            mirror_asset_cw20_amount.checked_sub(mirror_asset_cw20_balance)?;
        let terraswap_pair_asset_info = create_terraswap_cw20_uusd_pair_asset_info(
            position_info.mirror_asset_cw20_addr.as_str(),
        );
        let terraswap_pair_info = terraswap::querier::query_pair_info(
            &deps.querier,
            context.terraswap_factory_addr,
            &terraswap_pair_asset_info,
        )?;
        let reverse_simulation_response: terraswap::pair::ReverseSimulationResponse =
            deps.querier.query_wasm_smart(
                &terraswap_pair_info.contract_addr,
                &terraswap::pair::QueryMsg::ReverseSimulation {
                    ask_asset: terraswap::asset::Asset {
                        amount: mirror_asset_cw20_ask_amount,
                        info: terraswap::asset::AssetInfo::Token {
                            contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
                        },
                    },
                },
            )?;
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
        contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
            contract: context.mirror_mint_addr.to_string(),
            amount: position_response.asset.amount,
            msg: to_binary(&mirror_protocol::mint::Cw20HookMsg::Burn {
                position_idx: position_info.cdp_idx,
            })?,
        })?,
        funds: vec![],
    });
    response = response.add_message(burn_minted_mirror_asset);

    let withdraw_collateral = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: position_info.mirror_asset_cw20_addr.into_string(),
        msg: to_binary(&mirror_protocol::mint::ExecuteMsg::Withdraw {
            collateral: None,
            position_idx: position_info.cdp_idx,
        })?,
        funds: vec![],
    });
    response = response.add_message(withdraw_collateral);

    Ok(response)
}

pub fn stake_terraswap_lp_tokens(
    deps: Deps,
    env: Env,
    context: Context,
    lp_token_cw20_addr: String,
    stake_via_spectrum: bool,
) -> StdResult<Response> {
    let lp_token_amount = terraswap::querier::query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&lp_token_cw20_addr)?,
        env.contract.address,
    )?;
    let position_info = POSITION_INFO.load(deps.storage)?;
    if stake_via_spectrum {
        Ok(
            Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_cw20_addr,
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: context.spectrum_mirror_farms_addr.to_string(),
                    amount: lp_token_amount,
                    msg: to_binary(&spectrum_protocol::mirror_farm::Cw20HookMsg::bond {
                        asset_token: position_info.mirror_asset_cw20_addr.to_string(),
                        compound_rate: Some(Decimal::one()),
                        staker_addr: None,
                    })?,
                })?,
                funds: vec![],
            })),
        )
    } else {
        Ok(
            Response::new().add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_cw20_addr,
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: context.mirror_staking_addr.to_string(),
                    amount: lp_token_amount,
                    msg: to_binary(&mirror_protocol::staking::Cw20HookMsg::Bond {
                        asset_token: position_info.mirror_asset_cw20_addr.to_string(),
                    })?,
                })?,
                funds: vec![],
            })),
        )
    }
}

pub fn pair_ust_with_mirror_asset_and_stake(
    deps: Deps,
    env: Env,
    context: Context,
    mirror_asset_amount: Uint128,
    stake_via_spectrum: bool,
) -> StdResult<Response> {
    let position_info = POSITION_INFO.load(deps.storage)?;
    let mut response = Response::new();

    // Find uusd amount to pair with mAsset of quantity `mirror_asset_amount`.
    let terraswap_pair_asset_info = create_terraswap_cw20_uusd_pair_asset_info(
        &position_info.mirror_asset_cw20_addr.to_string(),
    );
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        context.terraswap_factory_addr,
        &terraswap_pair_asset_info,
    )?;
    let terraswap_pair_contract_addr =
        deps.api.addr_validate(&terraswap_pair_info.contract_addr)?;
    let pool_mirror_asset_balance = terraswap_pair_asset_info[0].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr.clone(),
    )?;
    let pool_uusd_balance = terraswap_pair_asset_info[1].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr,
    )?;
    let uusd_amount_to_provide_liquidity =
        mirror_asset_amount.multiply_ratio(pool_uusd_balance, pool_mirror_asset_balance);

    // Allow Terraswap mAsset-UST pair contract to transfer mAsset tokens from us.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
        msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pair_info.contract_addr.clone(),
            amount: mirror_asset_amount,
            expires: None,
        })?,
        funds: vec![],
    }));

    // Provide liquidity to Terraswap mAsset-UST pool.
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair_info.contract_addr,
        msg: to_binary(&terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: position_info.mirror_asset_cw20_addr.to_string(),
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
            receiver: None,
        })?,
        funds: vec![Coin {
            denom: String::from("uusd"),
            amount: uusd_amount_to_provide_liquidity,
        }],
    }));

    // Stake Terraswap LP tokens to Mirror Long Farm or Spectrum Mirror Vault.
    response = response.add_message(create_internal_execute_message(
        &env,
        InternalExecuteMsg::StakeTerraswapLpTokens {
            lp_token_cw20_addr: terraswap_pair_info.liquidity_token,
            stake_via_spectrum,
        },
    ));

    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPositionInfo {} => to_binary(&(POSITION_INFO.load(deps.storage)?)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
