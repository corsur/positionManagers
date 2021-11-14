use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Structures commonly shared by Aperture contracts.

/// Action enum that represents what users can do to each strategy.
/// For instance, users can open a position, which is represented by the
/// OpenPosition variant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyAction {
    OpenPosition {},
    ClosePosition {},
    IncreasePosition {},
    DecreasePosition {},
}

/// Enum represents individual strategy. When new strategies are added,
/// they must be added here to be used by smart contracts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyType {
    Anchor {},
    DeltaNeutral {},
}
