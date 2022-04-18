use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    common::{Action, ChainId, Position, PositionId},
    delta_neutral_position::PositionInfoResponse,
};

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
    pub min_open_uusd_amount: Uint128,
    // The minimum uusd amount eligible for delta-neutral reinvestment.
    pub min_reinvest_uusd_amount: Uint128,
    pub fee_collection_config: FeeCollectionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    SendOpenPositionToPositionContract {
        position: Position,
        params: DeltaNeutralParams,
        uusd_amount: Uint128,
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
        terra_manager_addr: Option<String>,
        delta_neutral_position_code_id: Option<u64>,
    },
    // Can only be called by admin.
    UpdateFeeCollectionConfig {
        fee_collection_config: FeeCollectionConfig,
    },
    // Can only be called by admin.
    UpdatePositionOpenMirrorAssetList {
        mirror_assets: Vec<String>,
        allowed: bool,
    },
    // Can only be called by admin.
    // If a certain mAsset will be delisted soon, the admin can preemptively add the mAsset to this list so the controller can close the CDPs involving the mAsset before the delist takes effect.
    // The admin should only utilize this feature when the delisting is certain to happen, either:
    // (1) Due to an announced corporate event with a scheduled date (merger, stock split, etc.);
    // (2) Mirror governance voted to delist an mAsset.
    // The admin should add the mAsset to this list no earlier than ~1 business day before the scheduled effective timestamp. For example, the admin should ideally wait until Mirror governance vote successfully passes (but has not yet executed).
    AddShouldPreemptivelyCloseCdpMirrorAssetList {
        mirror_assets: Vec<String>,
    },
    // Can only be called by admin.
    UpdateContext {
        controller: Option<String>,
        mirror_collateral_oracle_addr: Option<String>,
        mirror_oracle_addr: Option<String>,
        collateral_ratio_safety_margin: Option<Decimal>,
        min_open_uusd_amount: Option<Uint128>,
        min_reinvest_uusd_amount: Option<Uint128>,
    },
    // Can only be called by this contract itself.
    Internal(InternalExecuteMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {
    pub fee_collection_config: FeeCollectionConfig,
    pub position_open_allowed_mirror_assets: Vec<String>,
}

/// Represents position ids of the range [start, end) on the chain identified by `chain_id`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionRange {
    pub chain_id: ChainId,
    pub start: PositionId,
    pub end: PositionId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetPositionContractAddr {
        position: Position,
    },
    BatchGetPositionInfo {
        positions: Option<Vec<Position>>,
        ranges: Option<Vec<PositionRange>>,
    },
    GetContext {},
    GetAdminConfig {},
    // Returns CheckMirrorAssetAllowlistResponse.
    CheckMirrorAssetAllowlist {
        mirror_assets: Vec<String>,
    },
    // Returns ShouldCallRebalanceAndReinvestResponse.
    ShouldCallRebalanceAndReinvest {
        position: Position,
        // Should call when mAsset net amount is above this threshold, i.e. `abs(longAmount - shortAmount) / longAmount > mirror_asset_net_amount_tolerance_ratio`.
        mirror_asset_net_amount_tolerance_ratio: Decimal,
        // Should call when liquid uusd / position value is above this threshold.
        liquid_uusd_threshold_ratio: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ShouldCallRebalanceAndReinvestResponse {
    pub should_call: bool,
    pub position_contract: Addr,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct CheckMirrorAssetAllowlistResponse {
    // Whether the requested mirror assets are allowed to open positions with, in the order of the input mAsset array.
    pub allowed: Vec<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchGetPositionInfoResponseItem {
    pub position: Position,
    pub contract: Addr,
    // If `position.chain_id` is TERRA_CHAIN, then this will be populated with `Some(position holder address)`; otherwise, None.
    pub holder: Option<String>,
    pub info: PositionInfoResponse,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchGetPositionInfoResponse {
    pub items: Vec<BatchGetPositionInfoResponseItem>,
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
    pub min_open_uusd_amount: Uint128,
    pub min_reinvest_uusd_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct FeeCollectionConfig {
    // Performance fee rate. We periodically collect fees from positions; this fee rate is applied to the net uusd value gain since the previous fee collection.
    pub performance_rate: Decimal,
    // Flat service fee in uusd for opening a position when oracle price is stale (usually off-market).
    pub off_market_position_open_service_fee_uusd: Uint128,
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
    // If `Some(true)`, then allow the position to open even if the current oracle price is not fresh.
    // An off-market position open service fee will be deducted immediately from the deposit.
    // A non-fresh oracle price usually indicates that the off-chain market for the mAsset is currently closed; however, during active market hours there is a possibility of oracle provider delay making the oracle price stale.
    // Funds will be deposited to Anchor Earn at the time of position open; later when oracle price becomes fresh, the controller is able to trigger actual DN position setup by invoking RebalanceAndReinvest.
    pub allow_off_market_position_open: Option<bool>,
}
