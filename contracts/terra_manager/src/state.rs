use aperture_common::common::{PositionId, PositionKey, Strategy, StrategyId, StrategyMetadata};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map, U64Key};

pub const OWNER: Item<Addr> = Item::new("owner");

pub type StrategyIdKey = U64Key;
pub const NEXT_STRATEGY_ID: Item<StrategyId> = Item::new("next_strategy_id");
pub const STRATEGY_ID_TO_METADATA_MAP: Map<StrategyIdKey, StrategyMetadata> =
    Map::new("strategy_id_to_metadata_map");

pub const NEXT_POSITION_ID: Item<PositionId> = Item::new("next_position_id");
pub const POSITION_TO_STRATEGY_MAP: Map<PositionKey, Strategy> =
    Map::new("position_to_strategy_map");

pub fn get_strategy_id_key(strategy_id: StrategyId) -> StrategyIdKey {
    StrategyIdKey::from(strategy_id.u64())
}
