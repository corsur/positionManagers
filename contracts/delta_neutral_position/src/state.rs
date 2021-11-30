use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PositionInfo {
    pub cdp_idx: Uint128,
    pub mirror_asset_cw20_addr: Addr,
}

pub const MANAGER: Item<Addr> = Item::new("manager");
pub const POSITION_INFO: Item<PositionInfo> = Item::new("position_info");
