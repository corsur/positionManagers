use aperture_common::{
    delta_neutral_position::TerraswapPoolInfo, delta_neutral_position_manager::Context,
};
use cosmwasm_std::{
    to_binary, Addr, CanonicalAddr, CosmosMsg, Decimal, Deps, StdResult, Uint128, WasmMsg,
};
use cw_storage_plus::Map;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Spectrum Mirror Farm pool info.
// Copied from https://github.com/spectrumprotocol/contracts/blob/c6d95b8e853b16c94f98db60695c299c0d308fce/contracts/farms/spectrum_mirror_farm/src/state.rs#L83
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SpectrumPoolInfo {
    // LP token
    pub staking_token: CanonicalAddr,

    // total auto-compound share in the pool
    pub total_auto_bond_share: Uint128,

    // total auto-stake share in the pool
    pub total_stake_bond_share: Uint128,

    // LP amount for auto-stake
    pub total_stake_bond_amount: Uint128,

    // distribution weight
    pub weight: u32,

    // current MIR reward share for the pool
    pub farm_share: Uint128,

    // index to reconcile with state.spec_share_index
    // (state.spec_share_index - pool_info.state_spec_share_index) * pool_info.weight = additional SPEC rewards for this pool
    pub state_spec_share_index: Decimal,

    // total MIR rewards in share per total_stake_bond_share
    pub farm_share_index: Decimal,

    // additional SPEC rewards allocated for auto-compound per total_auto_bond_share
    pub auto_spec_share_index: Decimal,

    // additional SPEC rewards allocated for auto-stake per total_stake_bond_share
    pub stake_spec_share_index: Decimal,

    // for MIR pool: number of MIR to reinvest
    // for non-MIR pool: number of UST to reinvest
    pub reinvest_allowance: Uint128,
}

// Copied from https://github.com/spectrumprotocol/contracts/blob/c6d95b8e853b16c94f98db60695c299c0d308fce/contracts/farms/spectrum_mirror_farm/src/state.rs#L121
impl SpectrumPoolInfo {
    pub fn calc_auto_bond_share(&self, auto_bond_amount: Uint128, lp_balance: Uint128) -> Uint128 {
        let total_auto_bond_amount = lp_balance
            .checked_sub(self.total_stake_bond_amount)
            .unwrap();
        if self.total_auto_bond_share.is_zero() || total_auto_bond_amount.is_zero() {
            auto_bond_amount
        } else {
            auto_bond_amount.multiply_ratio(self.total_auto_bond_share, total_auto_bond_amount)
        }
    }

    pub fn calc_user_auto_balance(&self, lp_balance: Uint128, auto_bond_share: Uint128) -> Uint128 {
        if self.total_auto_bond_share.is_zero() {
            Uint128::zero()
        } else {
            lp_balance
                .checked_sub(self.total_stake_bond_amount)
                .unwrap()
                .multiply_ratio(auto_bond_share, self.total_auto_bond_share)
        }
    }
}

// Read Spectrum pool info for `mirror_asset_cw20_addr`.
pub fn get_spectrum_mirror_pool_info(
    deps: Deps,
    spectrum_mirror_farms_addr: &Addr,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<SpectrumPoolInfo> {
    let pool_info_map: Map<&[u8], SpectrumPoolInfo> = Map::new("pool_info");
    Ok(pool_info_map
        .query(
            &deps.querier,
            spectrum_mirror_farms_addr.clone(),
            deps.api
                .addr_canonicalize(mirror_asset_cw20_addr.as_str())?
                .as_slice(),
        )?
        .unwrap())
}

// Obtain LP balance of the whole Spectrum pool, i.e. the amount of LP tokens staked in Mirror long farm by the Spectrum farm contract.
pub fn get_spectrum_mirror_lp_balance(
    deps: Deps,
    mirror_staking_addr: &Addr,
    spectrum_mirror_farms_addr: &Addr,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<Uint128> {
    let mirror_reward_info: mirror_protocol::staking::RewardInfoResponse =
        deps.querier.query_wasm_smart(
            mirror_staking_addr,
            &mirror_protocol::staking::QueryMsg::RewardInfo {
                staker_addr: spectrum_mirror_farms_addr.to_string(),
                asset_token: Some(mirror_asset_cw20_addr.to_string()),
            },
        )?;
    Ok(mirror_reward_info.reward_infos[0].bond_amount)
}

// Simulate unbonding `withdraw_lp_token_amount` from Spectrum Mirror Farm and return the remaining amount of LP tokens after the unbond.
// If the current bonded LP token amount is `x`, after unbonding `withdraw_lp_token_amount` amount of LP tokens, the remaining LP token amount may be less than `x - withdraw_lp_token_amount`.
// The amount of auto-compound shares is `spectrum_auto_compound_share_amount` prior to the simulated withdrawal.
pub fn simulate_spectrum_mirror_farm_unbond(
    spectrum_mirror_pool_lp_balance: Uint128,
    mut spectrum_pool_info: SpectrumPoolInfo,
    spectrum_auto_compound_share_amount: Uint128,
    withdraw_lp_token_amount: Uint128,
) -> StdResult<Uint128> {
    // Simulate unbond of `withdraw_lp_token_amount` and calculate remaining bonded LP token amount.
    // Reference: https://github.com/spectrumprotocol/contracts/blob/c6d95b8e853b16c94f98db60695c299c0d308fce/contracts/farms/spectrum_mirror_farm/src/bond.rs#L377
    let mut auto_bond_share = spectrum_pool_info
        .calc_auto_bond_share(withdraw_lp_token_amount, spectrum_mirror_pool_lp_balance);
    if spectrum_pool_info.calc_user_auto_balance(spectrum_mirror_pool_lp_balance, auto_bond_share)
        < withdraw_lp_token_amount
    {
        auto_bond_share += Uint128::new(1u128);
    }
    spectrum_pool_info.total_auto_bond_share = spectrum_pool_info
        .total_auto_bond_share
        .checked_sub(auto_bond_share)?;
    Ok(spectrum_pool_info.calc_user_auto_balance(
        spectrum_mirror_pool_lp_balance - withdraw_lp_token_amount,
        spectrum_auto_compound_share_amount - auto_bond_share,
    ))
}

// Check whether the Spectrum Mirror farm for `mirror_asset_cw20_addr` exists.
pub fn check_spectrum_mirror_farm_existence(
    deps: Deps,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
) -> bool {
    // Spectrum Mirror farm pool information is stored in a map with:
    // - namespace: "pool_info".
    // - key: mAsset address in canonical form.
    // In raw storage, map keys are prefixed with a two-byte namespace length followed by the namespace itself.
    // See https://docs.rs/cosmwasm-storage/0.16.4/src/cosmwasm_storage/length_prefixed.rs.html.
    // To get the length-prefixed namespace, `to_length_prefixed("pool_info".as_bytes())` should return `"\u{0}\u{9}pool_info".as_bytes()`.
    // The "test" verify_length_prefix() below verifies this behavior.
    // Here, we are only interested in the existence of a specific key, so we don't try to deserialize the value.
    static PREFIX: &[u8] = "\u{0}\u{9}pool_info".as_bytes();
    let query_key = concat(
        PREFIX,
        deps.api
            .addr_canonicalize(mirror_asset_cw20_addr.as_str())
            .unwrap()
            .as_slice(),
    );
    deps.querier
        .query_wasm_raw(context.spectrum_mirror_farms_addr.to_string(), query_key)
        .unwrap()
        .is_some()
}

// Concatenates two byte slices.
#[inline]
fn concat(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut result = a.to_vec();
    result.extend_from_slice(b);
    result
}

// Verify behavior of encode_length() in cosmwasm-storage.
// See https://docs.rs/cosmwasm-storage/0.16.4/src/cosmwasm_storage/length_prefixed.rs.html#32.
#[test]
fn verify_length_prefix() {
    let namespace = b"pool_info";
    let length_bytes = (namespace.len() as u32).to_be_bytes();
    assert_eq!(([length_bytes[2], length_bytes[3]]), [0, 9]);
    assert_eq!("\u{0}\u{9}".as_bytes(), [0, 9]);
}

// Unstake `withdraw_lp_token_amount` amount of LP token from Spectrum Mirror farm at `spectrum_mirror_farms_addr`,
// and then redeem the LP tokens at the Terraswap pool for mAsset (`mirror_asset_cw20_addr`) and UST.
pub fn unstake_lp_from_spectrum_and_withdraw_liquidity(
    terraswap_pool_info: &TerraswapPoolInfo,
    spectrum_mirror_farms_addr: &Addr,
    mirror_asset_cw20_addr: &Addr,
    withdraw_lp_token_amount: Uint128,
) -> Vec<CosmosMsg> {
    vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_mirror_farms_addr.to_string(),
            funds: vec![],
            msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                asset_token: mirror_asset_cw20_addr.to_string(),
                amount: withdraw_lp_token_amount,
            })
            .unwrap(),
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pool_info.lp_token_cw20_addr.to_string(),
            funds: vec![],
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: terraswap_pool_info.terraswap_pair_addr.to_string(),
                amount: withdraw_lp_token_amount,
                msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
            })
            .unwrap(),
        }),
    ]
}

#[test]
pub fn test_unstake_lp_from_spectrum_and_withdraw_liquidity() {
    let withdraw_lp_token_amount = Uint128::from(10u128);
    assert_eq!(
        unstake_lp_from_spectrum_and_withdraw_liquidity(
            &TerraswapPoolInfo {
                lp_token_amount: Uint128::from(100u128),
                lp_token_cw20_addr: String::from("lp_token_cw20"),
                lp_token_total_supply: Uint128::from(1000u128),
                terraswap_pair_addr: String::from("terraswap_pair"),
                terraswap_pool_mirror_asset_amount: Uint128::from(300u128),
                terraswap_pool_uusd_amount: Uint128::from(3000u128),
                spectrum_auto_compound_share_amount: Uint128::from(335195917u128),
            },
            &Addr::unchecked("spectrum_mirror_farms"),
            &Addr::unchecked("mirror_asset_cw20"),
            withdraw_lp_token_amount
        ),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: String::from("spectrum_mirror_farms"),
                funds: vec![],
                msg: to_binary(&spectrum_protocol::mirror_farm::ExecuteMsg::unbond {
                    asset_token: String::from("mirror_asset_cw20"),
                    amount: withdraw_lp_token_amount,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: String::from("lp_token_cw20"),
                funds: vec![],
                msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                    contract: String::from("terraswap_pair"),
                    amount: withdraw_lp_token_amount,
                    msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
                })
                .unwrap(),
            })
        ]
    )
}
