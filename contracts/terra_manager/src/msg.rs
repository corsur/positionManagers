use aperture_common::common::{StrategyAction, StrategyType, TokenInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

/// Terra manager is the entry point for a user to initiate an investment
/// transaction. It is responsible for locating the underlying contract strategy
/// manager address by utilizing Aperture registry, and delegate specific
/// business logic to the strategy manager.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// `RegisterInvestment` - Owner only. Message to write the pair
    /// <strategy_index, strategy_manager_addr> into storage.
    RegisterInvestment {
        strategy_type: StrategyType,
        strategy_manager_addr: String,
    },
    /// First time initiate a new strategy. A position id will be created.
    InitStrategy {
        strategy_type: StrategyType,
        action_type: StrategyAction,
        token_type: TokenInfo,
    },
    /// Update existing position for a strategy using the position id.
    UpdateStrategy {
        strategy_type: StrategyType,
        action_type: StrategyAction,
        token_type: TokenInfo,
        position_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetPositionInfo { position_id: u64 },
    GetStrategyManagerAddr { strategy_type: StrategyType },
}
