use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyAction {
    OpenPosition {},
    ClosePosition {},
    IncreasePosition {},
    DecreasePosition {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyType {
    Anchor {},
    DeltaNeutral {},
}
