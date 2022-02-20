use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{Addr, Deps};

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
