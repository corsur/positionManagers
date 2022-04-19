use aperture_common::common::PositionKey;
use cosmwasm_bignumber::Uint256;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AdminConfig {
    pub admin: Addr,
    pub terra_manager: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Environment {
    pub anchor_ust_cw20_addr: Addr,
    pub anchor_market_addr: Addr,
}

pub const ADMIN_CONFIG: Item<AdminConfig> = Item::new("ac");
pub const ENVIRONMENT: Item<Environment> = Item::new("e");
pub const POSITION_TO_ANCHOR_UST_AMOUNT: Map<PositionKey, Uint256> = Map::new("ptaua");
