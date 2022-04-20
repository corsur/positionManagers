use aperture_common::{
    common::{Position, PositionKey},
    delta_neutral_position_manager::{AdminConfig, Context, FeeCollectionConfig},
};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

pub const CONTEXT: Item<Context> = Item::new("context");
pub const FEE_COLLECTION_CONFIG: Item<FeeCollectionConfig> = Item::new("fee_collection_config");
pub const ADMIN_CONFIG: Item<AdminConfig> = Item::new("admin_config");
pub const TMP_POSITION: Item<Position> = Item::new("tmp_position");
pub const POSITION_TO_CONTRACT_ADDR: Map<PositionKey, Addr> = Map::new("position_to_contract_addr");
pub const POSITION_OPEN_ALLOWED_MIRROR_ASSETS: Map<String, bool> = Map::new("poama");
pub const SHOULD_PREEMPTIVELY_CLOSE_CDP_MIRROR_ASSETS: Map<Addr, bool> = Map::new("spccma");
