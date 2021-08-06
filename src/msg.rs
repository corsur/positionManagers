use cosmwasm_std::{CosmosMsg, Decimal, HumanAddr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub anchor_ust_cw20_addr: HumanAddr,
    pub mirror_asset_cw20_addr: HumanAddr,
    pub mirror_collateral_oracle_addr: HumanAddr,
    pub mirror_lock_addr: HumanAddr,
    pub mirror_mint_addr: HumanAddr,
    pub mirror_staking_addr: HumanAddr,
    pub terraswap_factory_addr: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    DeltaNeutralInvest {
        collateral_ratio: Decimal,
    },
    Do {
        cosmos_messages: Vec<CosmosMsg>,
    },
    Receive {
        cw20_receive_msg: cw20::Cw20ReceiveMsg,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
}

