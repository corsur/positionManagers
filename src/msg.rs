use cosmwasm_std::HumanAddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub anchor_ust_cw20_addr: HumanAddr,
    pub mirror_asset_cw20_addr: HumanAddr,
    pub mirror_mint_addr: HumanAddr,
    pub mirror_staking_addr: HumanAddr,
    pub terraswap_factory_addr: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    DeltaNeutralInvest {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
}
