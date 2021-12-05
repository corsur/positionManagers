use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::Position;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub delta_neutral_position_code_id: u64,
    pub controller: String,
    pub min_uusd_amount: Uint128,
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
        action_data_binary: Option<Binary>,
        assets: Vec<terraswap::asset::Asset>,
    },
    Internal(InternalExecuteMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

/// Get basic information for a position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetPositionContractAddr { position: Position },
    GetContext {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
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
}

// Parameters of a delta-neutral position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DeltaNeutralParams {
    pub target_min_collateral_ratio: Decimal,
    pub target_max_collateral_ratio: Decimal,
    pub mirror_asset_cw20_addr: String,
}

/// Action enum that represents what users can do to each strategy.
/// For instance, users can open a position, which is represented by the
/// OpenPosition variant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Action {
    OpenPosition,
    ClosePosition,
    IncreasePosition,
    DecreasePosition { proportion: Decimal },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ActionData {
    pub action: Action,
    pub params: DeltaNeutralParams,
}
