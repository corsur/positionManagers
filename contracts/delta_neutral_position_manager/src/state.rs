use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map, U128Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Basic config to be stored in storage.
/// * `owner`: the owner of this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub delta_neutral_position_code_id: u64,
    pub context: Context,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const TMP_POSITION_ID: Item<u128> = Item::new("tmp_position_id");
pub const POSITIONS: Map<U128Key, Addr> = Map::new("positions");
