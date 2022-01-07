use aperture_common::{
    common::{Position, PositionKey},
    delta_neutral_position_manager::{Context, FeeCollectionConfig},
};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Basic config to be stored in storage.
/// * `owner`: the owner of this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub context: Context,
    pub fee_collection: FeeCollectionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AdminConfig {
    pub admin: Addr,
    pub manager: Addr,
    pub delta_neutral_position_code_id: u64,
    pub allow_position_increase: bool,
    pub allow_position_decrease: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const ADMIN_CONFIG: Item<AdminConfig> = Item::new("admin_config");
pub const TMP_POSITION: Item<Position> = Item::new("tmp_position");
pub const POSITION_TO_CONTRACT_ADDR: Map<PositionKey, Addr> = Map::new("position_to_contract_addr");
