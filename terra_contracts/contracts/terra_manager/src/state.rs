use aperture_common::common::{
    PositionId, PositionKey, StrategyId, StrategyLocation, StrategyMetadata,
};
use cosmwasm_std::{Addr, Decimal};
use cw_controllers::Admin;
use cw_storage_plus::{Item, Map, U128Key, U16Key, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const ADMIN: Admin = Admin::new("admin");
pub const WORMHOLE_TOKEN_BRIDGE_ADDR: Item<Addr> = Item::new("wormhole_token_bridge_addr");
pub const WORMHOLE_CORE_BRIDGE_ADDR: Item<Addr> = Item::new("wormhole_core_bridge_addr");

pub type StrategyIdKey = U64Key;
pub const NEXT_STRATEGY_ID: Item<StrategyId> = Item::new("next_strategy_id");
pub const STRATEGY_ID_TO_METADATA_MAP: Map<StrategyIdKey, StrategyMetadata> =
    Map::new("strategy_id_to_metadata_map");

pub const NEXT_POSITION_ID: Item<PositionId> = Item::new("next_position_id");
pub const POSITION_TO_STRATEGY_LOCATION_MAP: Map<PositionKey, StrategyLocation> =
    Map::new("position_to_strategy_location_map");
pub const POSITION_ID_TO_HOLDER: Map<U128Key, Addr> = Map::new("position_id_to_holder_map");
pub const HOLDER_POSITION_ID_PAIR_SET: Map<(Addr, U128Key), ()> =
    Map::new("holder_position_id_pair_set");

pub fn get_strategy_id_key(strategy_id: StrategyId) -> StrategyIdKey {
    StrategyIdKey::from(strategy_id.u64())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct CrossChainOutgoingFeeConfig {
    pub rate: Decimal,
    pub fee_collector_addr: Addr,
}
pub const CROSS_CHAIN_OUTGOING_FEE_CONFIG: Item<CrossChainOutgoingFeeConfig> =
    Item::new("cross_chain_outgoing_fee_config");

// Map from Wormhole chain id to the Aperture manager contract address (32 bytes) on the keyed chain.
pub const CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP: Map<U16Key, [u8; 32]> =
    Map::new("chain_id_to_aperture_manager_address_map");

// Map for storing hashes of completed Aperture instruction messages.
// This is used to ensure that each Aperture instruction message can only be processed at most once.
pub const COMPLETED_INSTRUCTIONS: Map<&[u8], bool> = Map::new("completed_instructions");
