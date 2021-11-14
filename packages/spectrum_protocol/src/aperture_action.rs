use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionMsg {
    Invest { amount: Uint128 },
    InvestMore { amount: Uint128 },
    Withdraw { args: String },
    Close {},
    Query { args: String },
}
