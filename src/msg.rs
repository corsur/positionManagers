use cosmwasm_std::{CosmosMsg, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub anchor_ust_cw20_addr: HumanAddr,
    pub mirror_collateral_oracle_addr: HumanAddr,
    pub mirror_lock_addr: HumanAddr,
    pub mirror_mint_addr: HumanAddr,
    pub mirror_oracle_addr: HumanAddr,
    pub mirror_staking_addr: HumanAddr,
    pub spectrum_mirror_farms_addr: HumanAddr,
    pub spectrum_staker_addr: HumanAddr,
    pub terraswap_factory_addr: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    ClaimShortSaleProceedsAndStake {
        cdp_idx: Uint128,
        mirror_asset_amount: Uint128,
        stake_via_spectrum: bool,
    },
    CloseShortPosition {
        cdp_idx: Uint128,
    },
    DeltaNeutralInvest {
        collateral_asset_amount: Uint128,
        collateral_ratio_in_percentage: Uint128,
        mirror_asset_to_mint_cw20_addr: HumanAddr,
    },
    Do {
        cosmos_messages: Vec<CosmosMsg>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}
