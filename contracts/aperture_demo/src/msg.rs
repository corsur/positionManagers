use cosmwasm_std::{Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub amadeus_addr: String,
    pub wormhole_token_bridge_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ClaimTokensFromWormholeAndDeltaNeutralInvest {
        vaa: Binary,
        collateral_ratio_in_percentage: Uint128,
        mirror_asset_cw20_addr: String,
    },
    DeltaNeutralInvest {
        collateral_ratio_in_percentage: Uint128,
        mirror_asset_cw20_addr: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WormholeTokenBridgeExecuteMsg {
    SubmitVaa {
        data: Binary,
    },
}