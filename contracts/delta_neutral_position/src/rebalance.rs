use std::cmp::Ordering;

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

// Brings the position back to delta-neutral. If it's impossible to achieve an exact long-short amount match, then achieve a slight net-long position.
// An example of such a situation: the mAsset is small-priced, and the Terraswap pair pool state is such that offering 1 extra uusd results in >1 extra mAsset returned.
pub fn achieve_delta_neutral(
    deps: Deps,
    env: &Env,
    context: &Context,
) -> StdResult<Vec<CosmosMsg>> {
    let mirror_asset_cw20_addr = MIRROR_ASSET_CW20_ADDR.load(deps.storage)?;
    let mut state = get_position_state(deps, env, context)?;
    let info = &state.terraswap_pool_info;
    let one = Uint128::from(1u128);

    // We first claim all available reward and exchange them for uusd.
    let (mut messages, uusd_increase_amount) = claim_and_increase_uusd_balance(deps, env, context)?;
    state.uusd_balance += uusd_increase_amount;

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
                    &state,
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
                    &state,
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
