use cosmwasm_std::{Addr, Binary, Decimal, Uint128, Uint64};
use cw_storage_plus::{U128Key, U32Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Structures commonly shared by Aperture contracts.

pub type ChainId = u32;
pub type PositionId = Uint128;
pub type StrategyId = Uint64;
pub type PositionKey = (U32Key, U128Key);

pub fn get_position_key(position: &Position) -> PositionKey {
    (
        U32Key::from(position.chain_id),
        U128Key::from(position.position_id.u128()),
    )
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
/// and on which chain is it located. Chain id would be needed because the same
/// protocol/strategy can exist on multiple chains. And increasingly, this is
/// becoming more popular.
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
        recipient: String,
    },
    IncreasePosition {
        data: Option<Binary>,
    },
    DecreasePosition {
        proportion: Decimal,
        recipient: String,
    },
}
