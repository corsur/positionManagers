use std::cmp::Ordering;

use aperture_common::delta_neutral_position::PositionState;
use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{to_binary, Coin, CosmosMsg, Decimal, Deps, Env, StdResult, Uint128, WasmMsg};

use crate::dex_util::{simulate_terraswap_swap, swap_cw20_token_for_uusd};
use crate::state::{CDP_IDX, MIRROR_ASSET_CW20_ADDR};
use crate::util::{
    find_unclaimed_mir_amount, find_unclaimed_spec_amount, get_cdp_uusd_lock_info_result,
    get_position_state, get_uusd_asset_from_amount, unstake_lp_and_withdraw_liquidity,
};

// Claim all available reward and redeem for uusd:
// (1) MIR reward from Mirror short farm.
// (2) SPEC reward from Spectrum Mirror long farm.
// (3) Unlocked short sale proceeds, e.g. two weeks after position open or the previous reinvest event.
pub fn claim_and_increase_uusd_balance(
    deps: Deps,
    env: &Env,
    context: &Context,
) -> StdResult<(Vec<CosmosMsg>, Uint128)> {
    let spec_reward = find_unclaimed_spec_amount(deps, env, context)?;
    let mir_reward = find_unclaimed_mir_amount(deps, env, context)?;
    let mut uusd_increase_amount = Uint128::zero();
    let mut messages = vec![];

    // Claim MIR / SPEC reward and swap them for uusd.
    if spec_reward > Uint128::zero() {
        // Mint SPEC tokens to ensure that emissable SPEC tokens are available for withdrawal.
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_gov_addr.to_string(),
            msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {})?,
            funds: vec![],
        }));

        // Claim SPEC reward.
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.spectrum_mirror_farms_addr.to_string(),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::withdraw {
                asset_token: None,
                farm_amount: None,
                spec_amount: None,
            })?,
            funds: vec![],
        }));

        // Swap SPEC reward for uusd.
        let (spec_swap_msg, uusd_return_amount) = swap_cw20_token_for_uusd(
            &deps.querier,
            &context.terraswap_factory_addr,
            &context.astroport_factory_addr,
            &context.spectrum_cw20_addr,
            spec_reward,
        )?;
        messages.push(spec_swap_msg);
        uusd_increase_amount += uusd_return_amount;
    }
    if mir_reward > Uint128::zero() {
        // Claim MIR reward.
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: context.mirror_staking_addr.to_string(),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::Withdraw { asset_token: None })?,
            funds: vec![],
        }));

        // Swap MIR for uusd.
        let (mir_swap_msg, uusd_return_amount) = swap_cw20_token_for_uusd(
            &deps.querier,
            &context.terraswap_factory_addr,
            &context.astroport_factory_addr,
            &context.mirror_cw20_addr,
            mir_reward,
        )?;
        messages.push(mir_swap_msg);
        uusd_increase_amount += uusd_return_amount;
    }

    // If there are any unlocked funds in the short farm, claim them.
    let position_lock_info_result = get_cdp_uusd_lock_info_result(deps, context);
    if let Ok(position_lock_info_response) = position_lock_info_result {
        if position_lock_info_response.unlock_time <= env.block.time.seconds() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: context.mirror_lock_addr.to_string(),
                msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
                    positions_idx: vec![CDP_IDX.load(deps.storage)?],
                })?,
                funds: vec![],
            }));
            uusd_increase_amount += position_lock_info_response.locked_amount;
        }
    }
    Ok((messages, uusd_increase_amount))
}

pub fn achieve_delta_neutral(
    deps: Deps,
    env: &Env,
    context: &Context,
) -> StdResult<Vec<CosmosMsg>> {
    let mut state = get_position_state(deps, env, context)?;

    // We first claim all available reward and exchange them for uusd.
    let (mut messages, uusd_increase_amount) = claim_and_increase_uusd_balance(deps, env, context)?;
    state.uusd_balance += uusd_increase_amount;

    messages.extend(achieve_delta_neutral_from_state(deps, context, &state)?);
    Ok(messages)
}

// Brings the position back to delta-neutral.
// Our goal here is to minimize the amount of LP tokens that have to be unstaked and liquidity withdrawn, as:
// (1) Re-staking the liquidity is subject to a 0.1% Spectrum protocol fee.
// (2) Performing swaps while holding LP tokens allows us to earn a portion of the 0.3% Terraswap fees.
//
// An alternative, much simpler approach would be to unstake all LP tokens and withdraw liquidity, and then bring the position to delta-neutral without worrying about how Terraswap price movement affects our long position amount.
pub fn achieve_delta_neutral_from_state(
    deps: Deps,
    context: &Context,
    state: &PositionState,
) -> StdResult<Vec<CosmosMsg>> {
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
    let info = &state.terraswap_pool_info;
    let mut messages = vec![];
    let one = Uint128::from(1u128);
    match state
        .mirror_asset_long_amount
        .cmp(&state.mirror_asset_short_amount)
    {
        Ordering::Greater => {
            // We are in a net long position.
            // First, we binary search for the least amount of lp tokens to withdraw such that swapping (mAsset balance + withdrawn mAsset) for uusd is sufficient to bring us back to neutral.
            let mut a = Uint128::zero();
            let mut b = info.lp_token_amount + one;
            while b > a + one {
                let withdraw_lp_token_amount = (a + b) >> 1;
                let fraction =
                    Decimal::from_ratio(withdraw_lp_token_amount, info.lp_token_total_supply);
                let withdrawn_mirror_asset_amount =
                    info.terraswap_pool_mirror_asset_amount * fraction;
                let pool_mirror_asset_amount_after_withdrawal =
                    info.terraswap_pool_mirror_asset_amount - withdrawn_mirror_asset_amount;
                let pool_mirror_asset_amount_after_swap = pool_mirror_asset_amount_after_withdrawal
                    + withdrawn_mirror_asset_amount
                    + state.mirror_asset_balance;
                let new_long_farm_mirror_asset_amount = pool_mirror_asset_amount_after_swap
                    * Decimal::from_ratio(
                        info.lp_token_amount - withdraw_lp_token_amount,
                        info.lp_token_total_supply - withdraw_lp_token_amount,
                    );
                if new_long_farm_mirror_asset_amount >= state.mirror_asset_short_amount {
                    a = withdraw_lp_token_amount;
                } else {
                    b = withdraw_lp_token_amount;
                }
            }

            if a > Uint128::zero() {
                // We determined that we need to withdraw `a` amount of LP tokens.
                let withdraw_lp_token_amount = a;
                messages.extend(unstake_lp_and_withdraw_liquidity(
                    state,
                    context,
                    &mirror_asset_cw20_addr,
                    withdraw_lp_token_amount,
                ));
                messages.push(
                    swap_cw20_token_for_uusd(
                        &deps.querier,
                        &context.terraswap_factory_addr,
                        &context.astroport_factory_addr,
                        &mirror_asset_cw20_addr,
                        info.terraswap_pool_mirror_asset_amount
                            * Decimal::from_ratio(
                                withdraw_lp_token_amount,
                                info.lp_token_total_supply,
                            )
                            + state.mirror_asset_balance,
                    )?
                    .0,
                );
            } else {
                // We determined that we don't have to withdraw any LP tokens to achieve delta-neutral.
                // Therefore, we perform a binary search for the least amount of mAsset (in contract balance) to swap for uusd.
                let mut a = Uint128::zero();
                let mut b = state.mirror_asset_balance + one;
                while b > a + one {
                    let offer_mirror_asset_amount = (a + b) >> 1;
                    let pool_mirror_asset_amount_after_swap =
                        info.terraswap_pool_mirror_asset_amount + offer_mirror_asset_amount;
                    let new_mirror_asset_long_amount = state.mirror_asset_balance
                        - offer_mirror_asset_amount
                        + pool_mirror_asset_amount_after_swap
                            * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply);
                    if new_mirror_asset_long_amount >= state.mirror_asset_short_amount {
                        a = offer_mirror_asset_amount;
                    } else {
                        b = offer_mirror_asset_amount;
                    }
                }
                messages.push(
                    swap_cw20_token_for_uusd(
                        &deps.querier,
                        &context.terraswap_factory_addr,
                        &context.astroport_factory_addr,
                        &mirror_asset_cw20_addr,
                        a,
                    )?
                    .0,
                );
            }
        }
        Ordering::Less => {
            // Note that the following two binary searches are slightly different from the two above.
            // Difference #1: terminating condition (`while a < b` below vs. `while b > a + one` above).
            // Difference #2: range update (`if long < short then a = m + one` below vs. `if long >= short then a = m` above).
            // The reason is that we want `a` to represent the least feasible value such that we can achieve exact neutral or slightly net long.

            // We are in a net short position.
            // First, we binary search for the least amount of lp tokens to withdraw such that swapping (uusd balance + withdrawn uusd) for mAsset is sufficient to bring us back to neutral.
            let mut a = Uint128::zero();
            let mut b = info.lp_token_amount + one;
            while a < b {
                let withdraw_lp_token_amount = (a + b) >> 1;
                let fraction =
                    Decimal::from_ratio(withdraw_lp_token_amount, info.lp_token_total_supply);
                let withdrawn_mirror_asset_amount =
                    info.terraswap_pool_mirror_asset_amount * fraction;
                let withdrawn_uusd_amount = info.terraswap_pool_uusd_amount * fraction;
                let pool_mirror_asset_amount_after_withdrawal =
                    info.terraswap_pool_mirror_asset_amount - withdrawn_mirror_asset_amount;
                let pool_uusd_amount_after_withdrawal =
                    info.terraswap_pool_uusd_amount - withdrawn_uusd_amount;
                let (_, pool_mirror_asset_amount_after_swap, return_mirror_asset_amount) =
                    simulate_terraswap_swap(
                        pool_uusd_amount_after_withdrawal,
                        pool_mirror_asset_amount_after_withdrawal,
                        withdrawn_uusd_amount + state.uusd_balance,
                    );
                let mirror_asset_long_farm_amount = pool_mirror_asset_amount_after_swap
                    * Decimal::from_ratio(
                        info.lp_token_amount - withdraw_lp_token_amount,
                        info.lp_token_total_supply - withdraw_lp_token_amount,
                    );
                let mirror_asset_long_amount = state.mirror_asset_balance
                    + withdrawn_mirror_asset_amount
                    + return_mirror_asset_amount
                    + mirror_asset_long_farm_amount;
                if mirror_asset_long_amount < state.mirror_asset_short_amount {
                    a = withdraw_lp_token_amount + one;
                } else {
                    b = withdraw_lp_token_amount;
                }
            }

            if a > Uint128::zero() {
                // We determined that we need to withdraw `a` amount of LP tokens.
                let withdraw_lp_token_amount = a;
                let offer_uusd_amount = info.terraswap_pool_uusd_amount
                    * Decimal::from_ratio(withdraw_lp_token_amount, info.lp_token_total_supply)
                    + state.uusd_balance;
                messages.extend(unstake_lp_and_withdraw_liquidity(
                    state,
                    context,
                    &mirror_asset_cw20_addr,
                    withdraw_lp_token_amount,
                ));
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: info.terraswap_pair_addr.clone(),
                    msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                        offer_asset: get_uusd_asset_from_amount(offer_uusd_amount),
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    })?,
                    funds: vec![Coin {
                        denom: String::from("uusd"),
                        amount: offer_uusd_amount,
                    }],
                }));
            } else {
                // We determined that we don't have to withdraw any LP tokens to achieve delta-neutral.
                // Therefore, we perform a binary search for the least amount of uusd (in contract balance) to swap for mAsset.
                let mut a = Uint128::zero();
                let mut b = state.uusd_balance + one;
                while a < b {
                    let offer_uusd_amount = (a + b) >> 1;
                    let (_, pool_mirror_asset_amount_after_swap, return_mirror_asset_amount) =
                        simulate_terraswap_swap(
                            info.terraswap_pool_uusd_amount,
                            info.terraswap_pool_mirror_asset_amount,
                            offer_uusd_amount,
                        );
                    let mirror_asset_long_amount = pool_mirror_asset_amount_after_swap
                        * Decimal::from_ratio(info.lp_token_amount, info.lp_token_total_supply)
                        + return_mirror_asset_amount
                        + state.mirror_asset_balance;
                    if mirror_asset_long_amount < state.mirror_asset_short_amount {
                        a = offer_uusd_amount + one;
                    } else {
                        b = offer_uusd_amount;
                    }
                }
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: info.terraswap_pair_addr.clone(),
                    msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                        offer_asset: get_uusd_asset_from_amount(a),
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    })?,
                    funds: vec![Coin {
                        denom: String::from("uusd"),
                        amount: a,
                    }],
                }));
            }
        }
        Ordering::Equal => {}
    }
    Ok(messages)
}

#[test]
fn test_achieve_delta_neutral_from_net_long_simple_swap() {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Timestamp};

    let terraswap_factory_addr = Addr::unchecked("mock_terraswap_factory");
    let astroport_factory_addr = Addr::unchecked("mock_astroport_factory");
    let cw20_token_addr = Addr::unchecked("mock_cw20_addr");
    let terraswap_pair_addr = Addr::unchecked("mock_terraswap_pair");
    let astroport_pair_addr = Addr::unchecked("mock_astroport_pair");
    let querier = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(10u128),
        Uint128::from(9u128),
        cw20_token_addr.to_string(),
        Uint128::from(1000000u128),
        Uint128::from(9000000u128),
    );
    let mut deps = cosmwasm_std::OwnedDeps {
        storage: cosmwasm_std::testing::MockStorage::default(),
        api: cosmwasm_std::testing::MockApi::default(),
        querier,
    };
    MIRROR_ASSET_CW20_ADDR
        .save(deps.as_mut().storage, &cw20_token_addr)
        .unwrap();
    CDP_IDX
        .save(deps.as_mut().storage, &Uint128::from(1u128))
        .unwrap();

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("this");
    env.block.time = Timestamp::from_seconds(12345);
    let context = Context {
        controller: Addr::unchecked("controller"),
        anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
        mirror_cw20_addr: Addr::unchecked("mirror_cw20"),
        spectrum_cw20_addr: Addr::unchecked("spectrum_cw20"),
        anchor_market_addr: Addr::unchecked("anchor_market"),
        mirror_collateral_oracle_addr: Addr::unchecked("mirror_collateral_oracle"),
        mirror_lock_addr: Addr::unchecked("mirror_lock"),
        mirror_mint_addr: Addr::unchecked("mirror_mint"),
        mirror_oracle_addr: Addr::unchecked("mirror_oracle"),
        mirror_staking_addr: Addr::unchecked("mirror_staking"),
        spectrum_gov_addr: Addr::unchecked("spectrum_gov"),
        spectrum_mirror_farms_addr: Addr::unchecked("spectrum_mirror_farms"),
        spectrum_staker_addr: Addr::unchecked("spectrum_staker"),
        terraswap_factory_addr: terraswap_factory_addr,
        astroport_factory_addr: astroport_factory_addr,
        collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
        min_open_uusd_amount: Uint128::from(500u128),
        min_reinvest_uusd_amount: Uint128::from(10u128),
    };

    let messages = achieve_delta_neutral(deps.as_ref(), &env, &context).unwrap();
    assert_eq!(
        messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("spectrum_gov"),
            msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {}).unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("spectrum_mirror_farms"),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::withdraw {
                asset_token: None,
                farm_amount: None,
                spec_amount: None,
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[2],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("spectrum_cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(5u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[3],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("mirror_staking"),
            msg: to_binary(&mirror_protocol::staking::ExecuteMsg::Withdraw { asset_token: None })
                .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[4],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("mirror_cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(3u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[5],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("mirror_lock"),
            msg: to_binary(&mirror_protocol::lock::ExecuteMsg::UnlockPositionFunds {
                positions_idx: vec![Uint128::from(1u128)]
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[6],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cw20_token_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(996996u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
}

#[test]
fn test_achieve_delta_neutral_from_net_short_simple_swap() {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Timestamp};

    let terraswap_factory_addr = Addr::unchecked("mock_terraswap_factory");
    let astroport_factory_addr = Addr::unchecked("mock_astroport_factory");
    let cw20_token_addr = Addr::unchecked("mock_cw20_addr");
    let terraswap_pair_addr = Addr::unchecked("mock_terraswap_pair");
    let astroport_pair_addr = Addr::unchecked("mock_astroport_pair");
    let querier = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(10u128),
        Uint128::from(9u128),
        cw20_token_addr.to_string(),
        Uint128::from(1000000u128),
        Uint128::from(9000000u128),
    );
    let mut deps = cosmwasm_std::OwnedDeps {
        storage: cosmwasm_std::testing::MockStorage::default(),
        api: cosmwasm_std::testing::MockApi::default(),
        querier,
    };
    MIRROR_ASSET_CW20_ADDR
        .save(deps.as_mut().storage, &cw20_token_addr)
        .unwrap();
    CDP_IDX
        .save(deps.as_mut().storage, &Uint128::from(1u128))
        .unwrap();

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("this");
    env.block.time = Timestamp::from_seconds(12345);
    let context = Context {
        controller: Addr::unchecked("controller"),
        anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
        mirror_cw20_addr: Addr::unchecked("mirror_cw20"),
        spectrum_cw20_addr: Addr::unchecked("spectrum_cw20"),
        anchor_market_addr: Addr::unchecked("anchor_market"),
        mirror_collateral_oracle_addr: Addr::unchecked("mirror_collateral_oracle"),
        mirror_lock_addr: Addr::unchecked("mirror_lock"),
        mirror_mint_addr: Addr::unchecked("mirror_mint"),
        mirror_oracle_addr: Addr::unchecked("mirror_oracle"),
        mirror_staking_addr: Addr::unchecked("mirror_staking"),
        spectrum_gov_addr: Addr::unchecked("spectrum_gov"),
        spectrum_mirror_farms_addr: Addr::unchecked("spectrum_mirror_farms"),
        spectrum_staker_addr: Addr::unchecked("spectrum_staker"),
        terraswap_factory_addr: terraswap_factory_addr,
        astroport_factory_addr: astroport_factory_addr,
        collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
        min_open_uusd_amount: Uint128::from(500u128),
        min_reinvest_uusd_amount: Uint128::from(10u128),
    };

    let state = PositionState {
        uusd_balance: Uint128::from(100000u128),
        uusd_long_farm: Uint128::from(9000u128),
        mirror_asset_short_amount: Uint128::from(2000u128),
        mirror_asset_balance: Uint128::from(10u128),
        mirror_asset_long_farm: Uint128::from(1000u128),
        mirror_asset_long_amount: Uint128::from(1010u128),
        collateral_anchor_ust_amount: Uint128::from(9000u128),
        collateral_uusd_value: Uint128::from(9900u128),
        mirror_asset_oracle_price: Decimal::from_ratio(10u128, 1u128),
        anchor_ust_oracle_price: Decimal::from_ratio(11u128, 10u128),
        terraswap_pool_info: aperture_common::delta_neutral_position::TerraswapPoolInfo {
            lp_token_amount: Uint128::from(1u128),
            lp_token_cw20_addr: String::from("lp_token"),
            lp_token_total_supply: Uint128::from(1000u128),
            terraswap_pair_addr: String::from("mock_terraswap_pair"),
            terraswap_pool_mirror_asset_amount: Uint128::from(1000000u128),
            terraswap_pool_uusd_amount: Uint128::from(9000000u128),
        },
    };

    let messages = achieve_delta_neutral_from_state(deps.as_ref(), &context, &state).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair_addr.to_string(),
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(8946u128)
                },
                belief_price: None,
                max_spread: None,
                to: None,
            })
            .unwrap(),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(8946u128),
            }],
        })
    );
}

#[test]
fn test_achieve_delta_neutral_from_net_short_withdraw_lp_and_swap() {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Timestamp};

    let terraswap_factory_addr = Addr::unchecked("mock_terraswap_factory");
    let astroport_factory_addr = Addr::unchecked("mock_astroport_factory");
    let cw20_token_addr = Addr::unchecked("mock_cw20_addr");
    let terraswap_pair_addr = Addr::unchecked("mock_terraswap_pair");
    let astroport_pair_addr = Addr::unchecked("mock_astroport_pair");
    let querier = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(10u128),
        Uint128::from(9u128),
        cw20_token_addr.to_string(),
        Uint128::from(1000000u128),
        Uint128::from(9000000u128),
    );
    let mut deps = cosmwasm_std::OwnedDeps {
        storage: cosmwasm_std::testing::MockStorage::default(),
        api: cosmwasm_std::testing::MockApi::default(),
        querier,
    };
    MIRROR_ASSET_CW20_ADDR
        .save(deps.as_mut().storage, &cw20_token_addr)
        .unwrap();
    CDP_IDX
        .save(deps.as_mut().storage, &Uint128::from(1u128))
        .unwrap();

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("this");
    env.block.time = Timestamp::from_seconds(12345);
    let context = Context {
        controller: Addr::unchecked("controller"),
        anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
        mirror_cw20_addr: Addr::unchecked("mirror_cw20"),
        spectrum_cw20_addr: Addr::unchecked("spectrum_cw20"),
        anchor_market_addr: Addr::unchecked("anchor_market"),
        mirror_collateral_oracle_addr: Addr::unchecked("mirror_collateral_oracle"),
        mirror_lock_addr: Addr::unchecked("mirror_lock"),
        mirror_mint_addr: Addr::unchecked("mirror_mint"),
        mirror_oracle_addr: Addr::unchecked("mirror_oracle"),
        mirror_staking_addr: Addr::unchecked("mirror_staking"),
        spectrum_gov_addr: Addr::unchecked("spectrum_gov"),
        spectrum_mirror_farms_addr: Addr::unchecked("spectrum_mirror_farms"),
        spectrum_staker_addr: Addr::unchecked("spectrum_staker"),
        terraswap_factory_addr: terraswap_factory_addr,
        astroport_factory_addr: astroport_factory_addr,
        collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
        min_open_uusd_amount: Uint128::from(500u128),
        min_reinvest_uusd_amount: Uint128::from(10u128),
    };

    let state = PositionState {
        uusd_balance: Uint128::from(1u128),
        uusd_long_farm: Uint128::from(9000u128),
        mirror_asset_short_amount: Uint128::from(2000u128),
        mirror_asset_balance: Uint128::from(10u128),
        mirror_asset_long_farm: Uint128::from(1000u128),
        mirror_asset_long_amount: Uint128::from(1010u128),
        collateral_anchor_ust_amount: Uint128::from(9000u128),
        collateral_uusd_value: Uint128::from(9900u128),
        mirror_asset_oracle_price: Decimal::from_ratio(10u128, 1u128),
        anchor_ust_oracle_price: Decimal::from_ratio(11u128, 10u128),
        terraswap_pool_info: aperture_common::delta_neutral_position::TerraswapPoolInfo {
            lp_token_amount: Uint128::from(1u128),
            lp_token_cw20_addr: String::from("lp_token"),
            lp_token_total_supply: Uint128::from(1000u128),
            terraswap_pair_addr: String::from("mock_terraswap_pair"),
            terraswap_pool_mirror_asset_amount: Uint128::from(1000000u128),
            terraswap_pool_uusd_amount: Uint128::from(9000000u128),
        },
    };

    let messages = achieve_delta_neutral_from_state(deps.as_ref(), &context, &state).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("spectrum_mirror_farms"),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                asset_token: cw20_token_addr.to_string(),
                amount: Uint128::from(1u128)
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("lp_token"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(1u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap()
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[2],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair_addr.to_string(),
            msg: to_binary(&terraswap::pair::ExecuteMsg::Swap {
                offer_asset: terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    },
                    amount: Uint128::from(9001u128)
                },
                belief_price: None,
                max_spread: None,
                to: None,
            })
            .unwrap(),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(9001u128),
            }],
        })
    );
}

#[test]
fn test_achieve_delta_neutral_from_net_long_withdraw_lp_and_swap() {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Timestamp};

    let terraswap_factory_addr = Addr::unchecked("mock_terraswap_factory");
    let astroport_factory_addr = Addr::unchecked("mock_astroport_factory");
    let cw20_token_addr = Addr::unchecked("mock_cw20_addr");
    let terraswap_pair_addr = Addr::unchecked("mock_terraswap_pair");
    let astroport_pair_addr = Addr::unchecked("mock_astroport_pair");
    let querier = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(10u128),
        Uint128::from(9u128),
        cw20_token_addr.to_string(),
        Uint128::from(1000000u128),
        Uint128::from(9000000u128),
    );
    let mut deps = cosmwasm_std::OwnedDeps {
        storage: cosmwasm_std::testing::MockStorage::default(),
        api: cosmwasm_std::testing::MockApi::default(),
        querier,
    };
    MIRROR_ASSET_CW20_ADDR
        .save(deps.as_mut().storage, &cw20_token_addr)
        .unwrap();
    CDP_IDX
        .save(deps.as_mut().storage, &Uint128::from(1u128))
        .unwrap();

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("this");
    env.block.time = Timestamp::from_seconds(12345);
    let context = Context {
        controller: Addr::unchecked("controller"),
        anchor_ust_cw20_addr: Addr::unchecked("anchor_ust_cw20"),
        mirror_cw20_addr: Addr::unchecked("mirror_cw20"),
        spectrum_cw20_addr: Addr::unchecked("spectrum_cw20"),
        anchor_market_addr: Addr::unchecked("anchor_market"),
        mirror_collateral_oracle_addr: Addr::unchecked("mirror_collateral_oracle"),
        mirror_lock_addr: Addr::unchecked("mirror_lock"),
        mirror_mint_addr: Addr::unchecked("mirror_mint"),
        mirror_oracle_addr: Addr::unchecked("mirror_oracle"),
        mirror_staking_addr: Addr::unchecked("mirror_staking"),
        spectrum_gov_addr: Addr::unchecked("spectrum_gov"),
        spectrum_mirror_farms_addr: Addr::unchecked("spectrum_mirror_farms"),
        spectrum_staker_addr: Addr::unchecked("spectrum_staker"),
        terraswap_factory_addr: terraswap_factory_addr,
        astroport_factory_addr: astroport_factory_addr,
        collateral_ratio_safety_margin: Decimal::from_ratio(3u128, 10u128),
        min_open_uusd_amount: Uint128::from(500u128),
        min_reinvest_uusd_amount: Uint128::from(10u128),
    };

    let state = PositionState {
        uusd_balance: Uint128::from(1u128),
        uusd_long_farm: Uint128::from(18000u128),
        mirror_asset_short_amount: Uint128::from(1000u128),
        mirror_asset_balance: Uint128::from(10u128),
        mirror_asset_long_farm: Uint128::from(2000u128),
        mirror_asset_long_amount: Uint128::from(1010u128),
        collateral_anchor_ust_amount: Uint128::from(9000u128),
        collateral_uusd_value: Uint128::from(9900u128),
        mirror_asset_oracle_price: Decimal::from_ratio(10u128, 1u128),
        anchor_ust_oracle_price: Decimal::from_ratio(11u128, 10u128),
        terraswap_pool_info: aperture_common::delta_neutral_position::TerraswapPoolInfo {
            lp_token_amount: Uint128::from(2u128),
            lp_token_cw20_addr: String::from("lp_token"),
            lp_token_total_supply: Uint128::from(1000u128),
            terraswap_pair_addr: String::from("mock_terraswap_pair"),
            terraswap_pool_mirror_asset_amount: Uint128::from(1000000u128),
            terraswap_pool_uusd_amount: Uint128::from(9000000u128),
        },
    };

    let messages = achieve_delta_neutral_from_state(deps.as_ref(), &context, &state).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("spectrum_mirror_farms"),
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                asset_token: cw20_token_addr.to_string(),
                amount: Uint128::from(1u128)
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("lp_token"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(1u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap()
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        messages[2],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cw20_token_addr.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pair_addr.to_string(),
                amount: Uint128::from(1010u128),
                msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
}
