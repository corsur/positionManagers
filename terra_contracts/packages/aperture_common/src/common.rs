/// This module defines data types commonly used in Aperture contracts.
use cosmwasm_std::{Addr, Binary, Decimal, Uint128, Uint256, Uint64};
use cw_storage_plus::{U128Key, U16Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Identifier of an Aperture-supported blockchain.
/// Since Aperture manager contracts on different chains communicate with each other via Wormhole,
/// we use the same chain ids as Wormhole, listed at https://docs.wormholenetwork.com/wormhole/contracts.
pub type ChainId = u16;

/// Identifier of an investment position on a specific supported chain.
/// An ordered pair (chain_id, position_id) uniquely identifies an investment position across the entire Aperture universe comprising all supported chains.
/// This position is owned by a user on the chain identified by `chain_id`.
pub type PositionId = Uint128;

/// Identifier of an investment strategy managed by Aperture on a specific chain.
/// An ordered pair (chain_id, strategy_id) uniquely identifies an investment strategy across the entire Aperture universe.
/// This strategy is managed by Aperture manager contract on the chain identified by `chain_id`.
/// For example (TERRA_CHAIN_ID, 1) represents strategy_id = 1 managed by the Aperture Terra Manager.
pub type StrategyId = Uint64;

/// Encoded cw_storage key representing an investment position (chain_id, position_id).
pub type PositionKey = (U16Key, U128Key);

pub fn get_position_key(position: &Position) -> PositionKey {
    (
        U16Key::from(position.chain_id),
        U128Key::from(position.position_id.u128()),
    )
}

pub fn get_position_key_from_tuple(position: &(ChainId, u128)) -> PositionKey {
    (U16Key::from(position.0), U128Key::from(position.1))
}

/// The pair (chain id, position id) can uniquely identify a position across
/// all chains.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Position {
    pub chain_id: ChainId,
    pub position_id: PositionId,
}

/// The strategy id and chain id can uniquely identify what strategy it is
/// and on which chain is it located.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Strategy {
    pub chain_id: ChainId,
    pub strategy_id: StrategyId,
}

/// Metadata describing a strategy.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StrategyMetadata {
    pub name: String,
    pub version: String,
    pub manager_addr: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrategyLocation {
    TerraChain(StrategyId),
    ExternalChain(ChainId),
}

/// Execute message that all strategy position manager contracts MUST be
/// able to handle.
/// Each strategy position manager MAY choose to add other variants to its
/// own ExecuteMsg.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrategyPositionManagerExecuteMsg {
    PerformAction {
        position: Position,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
}

/// Information about a swap-and-disburse instruction.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SwapAndDisburseInfo {
    // Encoded contract address of the desired token to disburse to the recipient (32-byte array).
    pub desired_token_addr: Binary,
    // The swap to the desired token must result in at least this amount of the output token; otherwise no swap is performed and the original token is directly disbursed to the recipient.
    pub minimum_amount: Uint256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Recipient {
    TerraChain {
        recipient: String,
    },
    ExternalChain {
        recipient_chain_id: u16,
        // Encoded recipient address (32-byte array).
        recipient_addr: Binary,
        // If Some(SwapAndDisburseInfo), swap wormhole-wrapped UST on the destination chain for `desired_token` and disburse that token to `recipient`,
        // but *only* if the swap results in an output amount of at least `minimum_amount`.
        // If `swap` is None, or if the swap results in a smaller output amount, then directly disburse wormhole-wrapped UST to `recipient`.
        swap_info: Option<SwapAndDisburseInfo>,
    },
}

/// Action enum that represents what users can do to each strategy.
/// For instance, users can open a position, which is represented by the
/// OpenPosition variant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    OpenPosition {
        data: Option<Binary>,
    },
    ClosePosition {
        recipient: Recipient,
    },
    IncreasePosition {
        data: Option<Binary>,
    },
    DecreasePosition {
        proportion: Decimal,
        recipient: Recipient,
    },
}
