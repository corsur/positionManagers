use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{Action, Position};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    // Administrator address.
    // Authorized to run `UpdateAdminConfig` and `CollectFees`.
    pub admin_addr: String,
    // Address of the Aperture Terra Manager.
    // Authorized to run `PerformAction`.
    pub terra_manager_addr: String,

    // Initial interest accural settings.
    pub accrual_rate_per_period: Decimal256,
    pub seconds_per_period: u64,

    // aUST cw20 address.
    pub anchor_ust_cw20_addr: String,
    // Anchor Market contract address.
    pub anchor_market_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Can only be called by the position holder through Terra manager.
    PerformAction {
        position: Position,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
    // Can only be called by admin.
    UpdateAdminConfig {
        admin_addr: Option<String>,
        terra_manager_addr: Option<String>,
        accrual_rate_per_period: Option<Decimal256>,
        seconds_per_period: Option<u64>,
    },
    // Can only be called by admin.
    CollectFees {
        uusd_amount: Uint128,
        recipient: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // Returns `PositionInfoResponse` with the uusd value of `position` at the current block's timestamp.
    GetPositionInfo { position: Position },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionInfoResponse {
    pub uusd_value: Uint256,
}
