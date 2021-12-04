use aperture_common::common::{ChainId, Position, Strategy};
use cosmwasm_std::{Binary, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static TERRA_CHAIN_ID: ChainId = 0;
pub static APERTURE_NFT: &str = "ApertureNFT";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub nft_addr: String,
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
    ExecuteStrategy {
        position: Position,
        action_data_binary: Option<Binary>,
        assets: Vec<terraswap::asset::Asset>,
    },
    CreateTerraNFTPosition {
        strategy: Strategy,
        action_data_binary: Option<Binary>,
        assets: Vec<terraswap::asset::Asset>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetStrategyMetadata { strategy_id: Uint64 },
}
