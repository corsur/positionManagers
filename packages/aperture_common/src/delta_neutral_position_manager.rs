use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{DeltaNeutralParams, StrategyAction, TokenInfo};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

/// List of actions available on this particular strategy. The specific enums
/// are inherited/copied from the Aperture common package.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Do {
        action: StrategyAction,
        token: TokenInfo,
        params: DeltaNeutralParams,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

/// Get basic information for a position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetPositionInfo { position_id: u64 },
}
