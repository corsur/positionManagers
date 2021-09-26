use cosmwasm_std::{CosmosMsg, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub controller: String,
    pub anchor_ust_cw20_addr: String,
    pub mirror_cw20_addr: String,
    pub spectrum_cw20_addr: String,
    pub anchor_market_addr: String,
    pub mirror_collateral_oracle_addr: String,
    pub mirror_lock_addr: String,
    pub mirror_mint_addr: String,
    pub mirror_oracle_addr: String,
    pub mirror_staking_addr: String,
    pub spectrum_gov_addr: String,
    pub spectrum_mirror_farms_addr: String,
    pub spectrum_staker_addr: String,
    pub terraswap_factory_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
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
        mirror_asset_to_mint_cw20_addr: String,
    },
    Do {
        cosmos_messages: Vec<CosmosMsg>,
    },
    Reinvest {},
    SetController {
        controller: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}
