use crate::common::{Action, ChainId, Position, PositionId, Recipient, Strategy, StrategyLocation};
use cosmwasm_std::{Binary, Decimal, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static TERRA_CHAIN_ID: ChainId = 3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub admin_addr: String,
    pub wormhole_core_bridge_addr: String,
    pub wormhole_token_bridge_addr: String,
    pub cross_chain_outgoing_fee_rate: Decimal,
    pub cross_chain_outgoing_fee_collector_addr: String,
}

/// Responsibilities of Aperture Terra manager:
/// (1) Manage information about positions opened (and owned) by Terra addresses.
/// (2) Manage information about positions that execute a Terra-chain strategy. Each position owner may be a Terra address or an external chain address.
/// (3) Provide endpoints for cross-chain communication / token transfers via Wormhole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Add strategy with the specified strategy manager address and metadata.
    /// A new, unique strategy_id is assigned to this strategy.
    /// Can only be called by the administrator.
    AddStrategy {
        name: String,
        version: String,
        manager_addr: String,
    },
    /// Remove the strategy associated with the specified identifier.
    /// Can only be called by the administrator.
    RemoveStrategy { strategy_id: Uint64 },
    /// Perform an action on an existing positions held by a Terra address.
    /// Can only be called by the position holder.
    ExecuteStrategy {
        position_id: PositionId,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
    /// Create a new position with the specified strategy.
    /// Can be called by any Terra address.
    CreatePosition {
        strategy: Strategy,
        data: Option<Binary>,
        assets: Vec<terraswap::asset::Asset>,
    },
    /// Registers the address of Aperture manager contract on an external chain.
    /// Can only be called by the administrator.
    RegisterExternalChainManager {
        chain_id: ChainId,
        // Wormhole encoded address of the Aperture manager contract (32-byte array).
        // We use `Binary` instead of `Vec<u8>` for a more compact (base-64) encoding.
        // See https://docs.rs/cosmwasm-std/0.16.4/cosmwasm_std/struct.Binary.html.
        aperture_manager_addr: Binary,
    },
    /// Processes a position action request instructed by an Aperture manager contract on an external chain.
    /// This handles actions on Terra strategy positions held by external chain addresses.
    /// Can be called by any Terra address.
    ProcessCrossChainInstruction {
        // VAA of an Aperture instruction message published by an external-chain Aperture manager.
        instruction_vaa: Binary,
        // VAAs of the accompanying token transfers, if any.
        token_transfer_vaas: Vec<Binary>,
    },
    /// Initiates cross-chain transfer via Wormhole token bridge to an external chain, after deduction of fees (e.g. 0.1% of the transfer amount).
    /// Although this can be called by any Terra address, this is only intended to be called by Aperture strategy contracts on Terra when outgoing cross-chain transfer is needed.
    /// Other callers will want to directly use Worhole token bridge to initiate the transfer without Aperture fees.
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
    // Returns `StrategyLocation`.
    GetStrategyLocationByPosition {
        position: Position,
    },
    // Returns an error if `instruction_vaa` cannot be parsed as a Wormhole VAA.
    // Otherwise, return whether `instruction_vaa` has already been processed by Aperture Terra Manager.
    // Note that this query does not check the validity of `instruction_vaa`.
    // If this query returns true, then `instruction_vaa` is valid and has been processed;
    // if false is returned, then `instruction_vaa` could be invalid, or valid but has not yet been processed.
    HasInstructionVaaBeenProcessed {
        instruction_vaa: Binary,
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
