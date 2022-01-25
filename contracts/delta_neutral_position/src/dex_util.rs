use std::convert::TryFrom;

use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal256, Deps, QuerierWrapper, StdError, StdResult, Uint128,
    Uint256, WasmMsg,
};
use terraswap::asset::PairInfo;

/// Returns an array comprising two AssetInfo elements, representing a Terraswap token pair where the first token is a cw20 with contract address
/// `cw20_token_addr` and the second token is the native "uusd" token. The returned array is useful for querying Terraswap for pair info.
///
/// # Arguments
/// * `cw20_token_addr` - Contract address of the specified cw20 token
pub fn create_terraswap_cw20_uusd_pair_asset_info(
    cw20_token_addr: &Addr,
) -> [terraswap::asset::AssetInfo; 2] {
    [
        terraswap::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.to_string(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

/// Returns an array comprising two AssetInfo elements, representing an Astroport token pair where the first token is a cw20 with contract address
/// `cw20_token_addr` and the second token is the native "uusd" token. The returned array is useful for querying Astroport for pair info.
///
/// # Arguments
/// * `cw20_token_addr` - Contract address of the specified cw20 token
fn create_astroport_cw20_uusd_pair_asset_info(
    cw20_token_addr: &Addr,
) -> [astroport::asset::AssetInfo; 2] {
    [
        astroport::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.clone(),
        },
        astroport::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}

#[test]
fn test_create_cw20_uusd_pair_asset_info() {
    let cw20_token_addr = Addr::unchecked("mock_addr");
    assert_eq!(
        create_astroport_cw20_uusd_pair_asset_info(&cw20_token_addr),
        [
            astroport::asset::AssetInfo::Token {
                contract_addr: cw20_token_addr.clone(),
            },
            astroport::asset::AssetInfo::NativeToken {
                denom: String::from("uusd"),
            }
        ]
    );
    assert_eq!(
        create_terraswap_cw20_uusd_pair_asset_info(&cw20_token_addr),
        [
            terraswap::asset::AssetInfo::Token {
                contract_addr: cw20_token_addr.to_string(),
            },
            terraswap::asset::AssetInfo::NativeToken {
                denom: String::from("uusd"),
            }
        ]
    );
}

/// Returns a Wasm execute message that swaps the cw20 token at address `cw20_token_addr` in the amount of `amount` for uusd via Terraswap or Astroport,
/// whichever returning more uusd.
///
/// # Arguments
///
/// * `querier` - Reference to a querier which is used to query Terraswap factory
/// * `terraswap_factory_addr` - Address of the Terraswap factory contract
/// * `astroport_factory_addr` - Address of the Astroport factory contract
/// * `cw20_token_addr` - Contract address of the cw20 token to be swapped
/// * `amount` - Amount of the cw20 token to be swapped
pub fn swap_cw20_token_for_uusd(
    querier: &QuerierWrapper,
    terraswap_factory_addr: &Addr,
    astroport_factory_addr: &Addr,
    cw20_token_addr: &Addr,
    amount: Uint128,
) -> StdResult<(CosmosMsg, Uint128)> {
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        querier,
        terraswap_factory_addr.clone(),
        &create_terraswap_cw20_uusd_pair_asset_info(cw20_token_addr),
    );
    let terraswap_uusd_return_amount = if let Ok(ref pair_info) = terraswap_pair_info {
        terraswap::querier::simulate(
            querier,
            Addr::unchecked(pair_info.contract_addr.clone()),
            &terraswap::asset::Asset {
                amount,
                info: terraswap::asset::AssetInfo::Token {
                    contract_addr: cw20_token_addr.to_string(),
                },
            },
        )
        .map_or(Uint128::zero(), |response| response.return_amount)
    } else {
        Uint128::zero()
    };

    let astroport_pair_info = astroport::querier::query_pair_info(
        querier,
        astroport_factory_addr.clone(),
        &create_astroport_cw20_uusd_pair_asset_info(cw20_token_addr),
    );
    let astroport_uusd_return_amount = if let Ok(ref pair_info) = astroport_pair_info {
        astroport::querier::simulate(
            querier,
            pair_info.contract_addr.clone(),
            &astroport::asset::Asset {
                amount,
                info: astroport::asset::AssetInfo::Token {
                    contract_addr: cw20_token_addr.clone(),
                },
            },
        )
        .map_or(Uint128::zero(), |response| response.return_amount)
    } else {
        Uint128::zero()
    };

    let (cw20_execute_msg, uusd_return_amount) =
        if terraswap_uusd_return_amount >= astroport_uusd_return_amount {
            (
                cw20::Cw20ExecuteMsg::Send {
                    contract: terraswap_pair_info?.contract_addr,
                    amount,
                    msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                        belief_price: None,
                        max_spread: None,
                        to: None,
                    })?,
                },
                terraswap_uusd_return_amount,
            )
        } else {
            (
                cw20::Cw20ExecuteMsg::Send {
                    contract: astroport_pair_info?.contract_addr.to_string(),
                    amount,
                    msg: to_binary(&astroport::pair::Cw20HookMsg::Swap {
                        belief_price: None,
                        max_spread: None,
                        to: None,
                    })?,
                },
                astroport_uusd_return_amount,
            )
        };
    Ok((
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cw20_token_addr.to_string(),
            msg: to_binary(&cw20_execute_msg)?,
            funds: vec![],
        }),
        uusd_return_amount,
    ))
}

#[test]
fn test_swap_cw20_token_for_uusd() {
    let terraswap_factory_addr = Addr::unchecked("mock_terraswap_factory");
    let astroport_factory_addr = Addr::unchecked("mock_astroport_factory");
    let cw20_token_addr = Addr::unchecked("mock_cw20_addr");
    let amount = Uint128::from(100u128);
    let terraswap_pair_addr = Addr::unchecked("mock_terraswap_pair");
    let astroport_pair_addr = Addr::unchecked("mock_astroport_pair");

    let querier_terraswap_better_rate = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(10u128),
        Uint128::from(9u128),
        cw20_token_addr.to_string(),
        Uint128::zero(),
        Uint128::zero(),
    );
    let querier_astroport_better_rate = crate::mock_querier::WasmMockQuerier::new(
        terraswap_factory_addr.to_string(),
        astroport_factory_addr.to_string(),
        terraswap_pair_addr.to_string(),
        astroport_pair_addr.to_string(),
        Uint128::from(8u128),
        Uint128::from(12u128),
        cw20_token_addr.to_string(),
        Uint128::zero(),
        Uint128::zero(),
    );
    assert_eq!(
        swap_cw20_token_for_uusd(
            &QuerierWrapper::new(&querier_terraswap_better_rate),
            &terraswap_factory_addr,
            &astroport_factory_addr,
            &cw20_token_addr,
            amount
        )
        .unwrap(),
        (
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_token_addr.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: terraswap_pair_addr.to_string(),
                    amount,
                    msg: to_binary(&terraswap::pair::Cw20HookMsg::Swap {
                        belief_price: None,
                        max_spread: None,
                        to: None,
                    })
                    .unwrap(),
                })
                .unwrap(),
                funds: vec![],
            }),
            Uint128::from(10u128)
        )
    );
    assert_eq!(
        swap_cw20_token_for_uusd(
            &QuerierWrapper::new(&querier_astroport_better_rate),
            &terraswap_factory_addr,
            &astroport_factory_addr,
            &cw20_token_addr,
            amount
        )
        .unwrap(),
        (
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_token_addr.to_string(),
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: astroport_pair_addr.to_string(),
                    amount,
                    msg: to_binary(&astroport::pair::Cw20HookMsg::Swap {
                        belief_price: None,
                        max_spread: None,
                        to: None,
                    })
                    .unwrap(),
                })
                .unwrap(),
                funds: vec![],
            }),
            Uint128::from(12u128)
        )
    );
}

/// Simulates a Terraswap pair contract's swap operation (a constant-product AMM w/ a fixed 0.3% commission).
/// Returns (offer_pool_amount_after_swap, ask_pool_amount_after_swap, return_amount).
///
/// We need to simulate swaps in hypothetical pool states, so we can't directly query Terraswap pair contract.
/// See https://github.com/terraswap/terraswap/blob/97cefa337798bdb0cba0327dd4152607839a5c77/contracts/terraswap_pair/src/contract.rs#L540.
///
/// # Arguments
///
/// * `offer_pool_amount` - amount of "offer asset" in the pool
/// * `ask_pool_amount` - amount of "ask asset" in the pool
/// * `offer_amount` - amount of "offer asset" being swapped for "ask asset"
pub fn simulate_terraswap_swap(
    offer_pool_amount: Uint128,
    ask_pool_amount: Uint128,
    offer_amount: Uint128,
) -> (Uint128, Uint128, Uint128) {
    let offer_pool_amount_after_swap = offer_pool_amount + offer_amount;
    let cp = offer_pool_amount.full_mul(ask_pool_amount);
    let one = Uint256::from(1u64);
    let commission_rate = Decimal256::from_ratio(3u64, 1000u64);
    let return_amount = (Decimal256::from_ratio(ask_pool_amount, one)
        - Decimal256::from_ratio(cp, offer_pool_amount_after_swap))
        * one;
    let return_amount = Uint128::try_from(return_amount - return_amount * commission_rate).unwrap();
    (
        offer_pool_amount_after_swap,
        ask_pool_amount - return_amount,
        return_amount,
    )
}

#[test]
fn test_simulate_terraswap_swap() {
    assert_eq!(
        simulate_terraswap_swap(
            Uint128::from(10000u128),
            Uint128::from(10000000u128),
            Uint128::from(100u128)
        ),
        (
            Uint128::from(10100u128),
            Uint128::from(9901288u128),
            Uint128::from(98712u128)
        )
    );
    assert_eq!(
        simulate_terraswap_swap(
            Uint128::from(395451850234u128),
            Uint128::from(317u128),
            Uint128::from(1u128)
        ),
        (
            Uint128::from(395451850235u128),
            Uint128::from(317u128),
            Uint128::zero()
        )
    );
    assert_eq!(
        simulate_terraswap_swap(
            Uint128::from(123456u128),
            Uint128::from(1234u128),
            Uint128::zero()
        ),
        (
            Uint128::from(123456u128),
            Uint128::from(1234u128),
            Uint128::zero()
        )
    );
}

/// Given a Terraswap pool state (a constant-product AMM w/ a fixed 0.3% commission), find the least amount of `offer asset` that can be swapped for at least `ask_amount` of the `ask asset`.
/// Due to rounding, Terraswap's implementation of reverse swap simulation may return insufficient `offer_amount`, that when swapped, resulting in less than the desired `ask_amount` being returned.
/// Here we use binary search to find the smallest possible `offer_amount` that results in a return amount >= `ask_amount`.
///
/// # Arguments
///
/// * `ask_pool_amount` - amount of "ask asset" in the pool
/// * `offer_pool_amount` - amount of "offer asset" in the pool
/// * `ask_amount` - amount of "ask asset" to return
pub fn compute_terraswap_offer_amount(
    ask_pool_amount: Uint128,
    offer_pool_amount: Uint128,
    ask_amount: Uint128,
) -> StdResult<Uint128> {
    if ask_amount >= ask_pool_amount {
        return Err(StdError::generic_err("insufficient liquidity"));
    }

    let offer_pool_amount = Uint256::from(offer_pool_amount);
    let ask_pool_amount = Uint256::from(ask_pool_amount);
    let ask_amount = Uint256::from(ask_amount);
    let cp = offer_pool_amount * ask_pool_amount;
    let one = Uint256::from(1u64);
    let commission_rate = Decimal256::from_ratio(3u64, 1000u64);

    let mut a = Uint256::zero();
    let mut b = Uint256::from(u128::MAX);
    while a < b {
        let offer_amount = (a + b) >> 1;
        let simulated_return_amount = (Decimal256::from_ratio(ask_pool_amount, one)
            - Decimal256::from_ratio(cp, offer_pool_amount + offer_amount))
            * one;
        let simulated_return_amount =
            simulated_return_amount - simulated_return_amount * commission_rate;
        if simulated_return_amount < ask_amount {
            a = offer_amount + one;
        } else {
            b = offer_amount;
        }
    }
    Ok(Uint128::try_from(a)?)
}

#[test]
fn test_compute_terraswap_offer_amount() {
    let ask_pool_amount = Uint128::from(135713121545u128);
    let offer_pool_amount = Uint128::from(241215454u128);
    let ask_amount = Uint128::from(1231231u128);
    let offer_amount =
        compute_terraswap_offer_amount(ask_pool_amount, offer_pool_amount, ask_amount).unwrap();
    let (_, _, simulated_return_amount) =
        simulate_terraswap_swap(offer_pool_amount, ask_pool_amount, offer_amount);
    assert!(simulated_return_amount >= ask_amount);

    assert_eq!(
        compute_terraswap_offer_amount(
            Uint128::from(100u128),
            Uint128::from(10u128),
            Uint128::from(100u128)
        ),
        Err(StdError::generic_err("insufficient liquidity"))
    );
    assert_eq!(
        compute_terraswap_offer_amount(
            Uint128::from(100u128),
            Uint128::from(10u128),
            Uint128::from(101u128)
        ),
        Err(StdError::generic_err("insufficient liquidity"))
    );
}

/// Obtains Terraswap mAsset-UST liquidity information.
/// Returns a tuple consisting of the following components:
/// * PairInfo for the mAsset-UST pair.
/// * Amount of mAsset in the pool.
/// * Amount of uusd in the pool.
///
/// # Arguments
///
/// * `deps` - reference to dependencies
/// * `terraswap_factory_addr` - terraswap factory contract address
/// * `mirror_asset_cw20_addr` - mAsset cw20 contract address
pub fn get_terraswap_mirror_asset_uusd_liquidity_info(
    deps: Deps,
    terraswap_factory_addr: &Addr,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<(PairInfo, Uint128, Uint128)> {
    let terraswap_pair_asset_info =
        create_terraswap_cw20_uusd_pair_asset_info(mirror_asset_cw20_addr);
    let terraswap_pair_info = terraswap::querier::query_pair_info(
        &deps.querier,
        terraswap_factory_addr.clone(),
        &terraswap_pair_asset_info,
    )?;
    let terraswap_pair_contract_addr = Addr::unchecked(terraswap_pair_info.contract_addr.clone());
    let pool_mirror_asset_amount = terraswap_pair_asset_info[0].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr.clone(),
    )?;
    let pool_uusd_amount = terraswap_pair_asset_info[1].query_pool(
        &deps.querier,
        deps.api,
        terraswap_pair_contract_addr,
    )?;
    Ok((
        terraswap_pair_info,
        pool_mirror_asset_amount,
        pool_uusd_amount,
    ))
}

#[test]
fn test_get_terraswap_mirror_asset_uusd_liquidity_info() {
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
        Uint128::from(1000u128),
        Uint128::from(100u128),
    );
    let deps = cosmwasm_std::OwnedDeps {
        storage: cosmwasm_std::testing::MockStorage::default(),
        api: cosmwasm_std::testing::MockApi::default(),
        querier,
    };
    assert_eq!(
        get_terraswap_mirror_asset_uusd_liquidity_info(
            deps.as_ref(),
            &terraswap_factory_addr,
            &cw20_token_addr
        )
        .unwrap(),
        (
            PairInfo {
                asset_infos: [
                    terraswap::asset::AssetInfo::Token {
                        contract_addr: cw20_token_addr.to_string(),
                    },
                    terraswap::asset::AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                ],
                contract_addr: terraswap_pair_addr.to_string(),
                liquidity_token: String::from("lp_token")
            },
            Uint128::from(1000u128),
            Uint128::from(100u128)
        )
    );
}
