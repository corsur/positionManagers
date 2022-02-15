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
    // - key: mAsset address.
    // In raw storage, map keys are prefixed with namespace. Here, we are only interested in the existence of a specific key, namely
    // `concat("pool_info", canonical address of mirror_asset_cw20_addr)`. Thus, we don't try to deserialize the value.
    static PREFIX_POOL_INFO: &[u8] = b"pool_info";
    let query_key = concat(
        PREFIX_POOL_INFO,
        deps.api
            .addr_canonicalize(mirror_asset_cw20_addr.as_str())
            .unwrap()
            .as_slice(),
    );
    deps.querier
        .query_wasm_raw(context.spectrum_mirror_farms_addr.to_string(), query_key)
        .unwrap_or_default()
        .is_some()
}

// Concatenates two byte slices.
#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
