use aperture_common::{
    delta_neutral_position::TerraswapPoolInfo, delta_neutral_position_manager::Context,
};
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Deps, Uint128, WasmMsg};

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
                terraswap_pool_uusd_amount: Uint128::from(3000u128)
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
