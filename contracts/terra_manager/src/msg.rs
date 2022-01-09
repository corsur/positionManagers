use aperture_common::common::{Action, ChainId, Position, PositionId, Strategy, StrategyLocation};
use cosmwasm_std::{Binary, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static TERRA_CHAIN_ID: ChainId = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {}

/// Terra manager is the entry point for a user to initiate an investment
/// transaction. It is responsible for locating the underlying contract strategy
/// manager address by utilizing Aperture registry, and delegate specific
/// business logic to the strategy manager.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Add strategy with the specified manager address and metadata.
    /// A new, unique identifier is assigned to this new strategy.
    ///
    /// Only contract owner may execute `AddStrategy`.
    AddStrategy {
        name: String,
        version: String,
        manager_addr: String,
    },
    /// Remove the strategy associated with the specified identifier.
    ///
    /// Only contract owner may execute `RemoveStrategy`.
    RemoveStrategy { strategy_id: Uint64 },
    /// Perform an action on an existing positions held by a Terra address.
    /// Only the position holder is able to call this.
    ExecuteStrategy {
        position: Position,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
    /// Create a new position with the specified strategy.
    CreatePosition {
        strategy: Strategy,
        data: Option<Binary>,
        assets: Vec<terraswap::asset::Asset>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetStrategyMetadata {
        strategy_id: Uint64,
    },
    GetNextPositionId {},
    GetHolderByTerraPositionId {
        position_id: PositionId,
    },
    GetTerraPositionsByHolder {
        holder: String,
        start_after: Option<PositionId>,
        limit: Option<usize>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct NextPositionIdResponse {
    pub next_position_id: PositionId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionHolderResponse {
    pub holder: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionsResponse {
    pub position_id_vec: Vec<PositionId>,
    pub strategy_location_vec: Vec<StrategyLocation>,
}
