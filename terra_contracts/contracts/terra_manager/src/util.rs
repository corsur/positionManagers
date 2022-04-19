use aperture_common::byte_util::extend_terra_address_to_32;
use cosmwasm_std::{Addr, Deps, StdResult};
use cw_storage_plus::Map;

use crate::state::WORMHOLE_CORE_BRIDGE_ADDR;

// Returns the sequence of the next message published by `emitter_address`.
// If `emitter_address` is the Wormhole token bridge contract, then the returned value is the sequence number for the next token transfer.
pub fn get_next_sequence(deps: Deps, emitter_address: &Addr) -> StdResult<u64> {
    // This map is only meant to be used to query Wormhole core bridge for the token bridge's sequence number in a type-safe way.
    // See https://docs.rs/cw-storage-plus/latest/cw_storage_plus/struct.Map.html#method.query for details.
    const WORMHOLE_SEQUENCE_MAP: Map<&[u8], u64> = Map::new("sequence");
    let emitter_key =
        extend_terra_address_to_32(&deps.api.addr_canonicalize(emitter_address.as_str())?);
    match WORMHOLE_SEQUENCE_MAP.query(
        &deps.querier,
        WORMHOLE_CORE_BRIDGE_ADDR.load(deps.storage)?,
        &emitter_key,
    ) {
        Ok(option_sequence) => Ok(option_sequence.unwrap_or(0u64)),
        Err(err) => Err(err),
    }
}
