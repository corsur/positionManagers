use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionInfo {
    pub cdp_idx: Uint128,
    pub mirror_asset_cw20_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TargetCollateralRatioRange {
    pub min: Decimal,
    pub max: Decimal,
}

impl TargetCollateralRatioRange {
    pub fn midpoint(&self) -> Decimal {
        (self.min + self.max) / 2u128.into()
    }
}

pub const MANAGER: Item<Addr> = Item::new("manager");
pub const POSITION_INFO: Item<PositionInfo> = Item::new("position_info");
pub const TARGET_COLLATERAL_RATIO_RANGE: Item<TargetCollateralRatioRange> =
    Item::new("target_collateral_ratio_range");
