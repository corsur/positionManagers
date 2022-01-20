use aperture_common::common::{
    PositionId, PositionKey, StrategyId, StrategyLocation, StrategyMetadata,
};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map, U128Key, U64Key};

pub const ADMIN: Item<Addr> = Item::new("admin");
pub const WORMHOLE_TOKEN_BRIDGE_ADDR: Item<Addr> = Item::new("wormhole_token_bridge_addr");

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
