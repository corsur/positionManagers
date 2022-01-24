use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{Action, Position};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    // Administrator address.
    pub admin_addr: String,
    // Aperture manager (Terra) address.
    pub terra_manager_addr: String,
    // Code id for delta-neutral position contract.
    pub delta_neutral_position_code_id: u64,
    // Controller address. Only this address is allowed to trigger rebalance & reinvest.
    pub controller: String,
    // Below are contract addresses that a delta-neutral position contract needs to interact with.
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
    pub astroport_factory_addr: String,
    // Each mAsset has a minimum required collateral ratio threshold.
    // The user-specified minimum target collateral ratio must exceed the threshold by at least `collateral_ratio_safety_margin`.
    // See also `DeltaNeutralParams` below for more context.
    pub collateral_ratio_safety_margin: Decimal,
    // The minimum allowed uusd amount when opening a delta-neutral position.
    pub min_delta_neutral_uusd_amount: Uint128,
    pub fee_collection_config: FeeCollectionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    SendOpenPositionToPositionContract {
        position: Position,
        params: DeltaNeutralParams,
        uusd_asset: terraswap::asset::Asset,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Can only be called by the position holder through Terra manager.
    PerformAction {
        position: Position,
        action: Action,
        assets: Vec<terraswap::asset::Asset>,
    },
    // Can be called by anyone to migrate position contracts to the current code id.
    MigratePositionContracts {
        // Can either specify a list of `positions` or the underlying `position_contracts`, or a mixture.
        // Specifying `position_contracts` directly saves gas since this avoids a position -> contract lookup.
        positions: Vec<Position>,
        position_contracts: Vec<String>,
    },
    // Can only be called by admin.
    UpdateAdminConfig {
        admin_addr: Option<String>,
        manager_addr: Option<String>,
        delta_neutral_position_code_id: Option<u64>,
    },
    // Can only be called by this contract itself.
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
    GetAdminConfig {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AdminConfig {
    pub admin: Addr,
    pub terra_manager: Addr,
    pub delta_neutral_position_code_id: u64,
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
    pub astroport_factory_addr: Addr,
    pub collateral_ratio_safety_margin: Decimal,
    pub min_delta_neutral_uusd_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct FeeCollectionConfig {
    // Performance fee rate. We periodically collect fees from positions; this fee rate is applied to the net uusd value gain since the previous fee collection.
    pub performance_rate: Decimal,
    // Address to which the collected fees go.
    pub collector_addr: String,
}

/// Parameters of a delta-neutral position specified by the user when opening this position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DeltaNeutralParams {
    // The target range of the collateral ratio.
    // Throughout the lifetime of this position, collalteral ratio is kept within this range through rebalancing.
    // Note that `target_min_collateral_ratio` >= mAsset_required_colleteral_ratio + `Context.collateral_ratio_safety_margin` must hold in order to open a position.
    // This is to ensure that the user cannot specify a `target_min_collateral_ratio` too close to the liquidation threshold.
    pub target_min_collateral_ratio: Decimal,
    pub target_max_collateral_ratio: Decimal,
    // The mAsset token used in this delta-neutral position.
    pub mirror_asset_cw20_addr: String,
}
