use crate::common::{Action, ChainId, Position, PositionId, Recipient, Strategy, StrategyLocation};
use cosmwasm_std::{Binary, Decimal, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static TERRA_CHAIN_ID: ChainId = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub wormhole_core_bridge_addr: String,
    pub wormhole_token_bridge_addr: String,
    pub cross_chain_outgoing_fee_rate: Decimal,
    pub cross_chain_outgoing_fee_collector_addr: String,
}

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
    RegisterExternalChainManager {
        chain_id: ChainId,
        aperture_manager_addr: Vec<u8>,
    },
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
    ProcessCrossChainInstruction {
        // VAA of an Aperture instruction message published by an external-chain Aperture manager.
        instruction_vaa: Binary,
        // VAAs of the accompanying token transfers.
        token_transfer_vaas: Vec<Binary>,
    },
    InitiateOutgoingTokenTransfer {
        assets: Vec<terraswap::asset::Asset>,
        recipient: Recipient,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // Returns `StrategyMetadata`.
    GetStrategyMetadata {
        strategy_id: Uint64,
    },
    // Returns `NextPositionIdResponse`.
    GetNextPositionId {},
    // Returns `PositionInfoResponse`.
    GetTerraPositionInfo {
        position_id: PositionId,
    },
    // Returns `PositionsResponse`.
    GetTerraPositionsByHolder {
        holder: String,
        start_after: Option<PositionId>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct NextPositionIdResponse {
    pub next_position_id: PositionId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionInfoResponse {
    pub holder: String,
    pub strategy_location: StrategyLocation,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionsResponse {
    pub position_id_vec: Vec<PositionId>,
    pub strategy_location_vec: Vec<StrategyLocation>,
}
