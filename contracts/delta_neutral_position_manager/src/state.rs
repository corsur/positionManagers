use aperture_common::{
    common::{Position, PositionKey},
    delta_neutral_position_manager::Context,
};
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Basic config to be stored in storage.
/// * `owner`: the owner of this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub delta_neutral_position_code_id: u64,
    pub context: Context,
    pub min_uusd_amount: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const TMP_POSITION: Item<Position> = Item::new("tmp_position");
pub const POSITION_TO_CONTRACT_ADDR: Map<PositionKey, Addr> = Map::new("position_to_contract_addr");
