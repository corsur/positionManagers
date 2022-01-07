use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{Action, Position};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub owner_addr: String,
    pub delta_neutral_position_code_id: u64,
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
    pub collateral_ratio_safety_margin: Decimal,
    pub min_delta_neutral_uusd_amount: Uint128,
    pub fee_collection_config: FeeCollectionConfig,
}

/// Internal execute messages that will only be processed if sent from the contract itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    SendOpenPositionToPositionContract {
        position: Position,
        params: DeltaNeutralParams,
        uusd_asset: terraswap::asset::Asset,
    },
}

/// List of actions available on this particular strategy. The specific enums
/// are inherited/copied from the Aperture common package.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    PerformAction {
        position: Position,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
    Internal(InternalExecuteMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

/// Get basic information from this position manager.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetPositionContractAddr { position: Position },
    GetContext {},
}

/// Contextual information for delta neutral position manager. It contains
/// addresses for contracts needed by this position manager along with
/// other necessary data.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Context {
    pub controller: Addr,
    pub anchor_ust_cw20_addr: Addr,
    pub mirror_cw20_addr: Addr,
    pub spectrum_cw20_addr: Addr,
    pub anchor_market_addr: Addr,
    pub mirror_collateral_oracle_addr: Addr,
    pub mirror_lock_addr: Addr,
    pub mirror_mint_addr: Addr,
    pub mirror_oracle_addr: Addr,
    pub mirror_staking_addr: Addr,
    pub spectrum_gov_addr: Addr,
    pub spectrum_mirror_farms_addr: Addr,
    pub spectrum_staker_addr: Addr,
    pub terraswap_factory_addr: Addr,
    pub collateral_ratio_safety_margin: Decimal,
    pub min_delta_neutral_uusd_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct FeeCollectionConfig {
    pub performance_rate: Decimal,
    pub treasury_addr: String,
}

/// Parameters of a delta-neutral position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DeltaNeutralParams {
    pub target_min_collateral_ratio: Decimal,
    pub target_max_collateral_ratio: Decimal,
    pub mirror_asset_cw20_addr: String,
}
