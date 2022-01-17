use aperture_common::common::PositionKey;
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AdminConfig {
    pub admin: Addr,
    pub manager: Addr,
    pub accrual_rate_per_block: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ShareInfo {
    pub exchange_rate: Decimal256,
    pub block_height: u64
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Environment {
    pub anchor_ust_cw20_addr: Addr,
    pub anchor_market_addr: Addr,
    pub wormhole_token_bridge_addr: Addr,
}

pub const ADMIN_CONFIG: Item<AdminConfig> = Item::new("admin_config");
pub const SHARE_INFO: Item<ShareInfo> = Item::new("share_info");
pub const TOTAL_SHARE_AMOUNT: Item<Uint256> = Item::new("total_share_amount");
pub const ENVIRONMENT: Item<Environment> = Item::new("environment");
pub const POSITION_TO_SHARE_AMOUNT: Map<PositionKey, Uint256> = Map::new("position_to_share_amount");
