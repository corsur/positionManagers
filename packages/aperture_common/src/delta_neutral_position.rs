use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::delta_neutral_position_manager::DeltaNeutralParams;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

/// Internal message to achieve better logic flow.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    ClaimAndIncreaseUusdBalance {},
    AchieveDeltaNeutral {},
    AchieveSafeCollateralRatio {},
    WithdrawFundsInUusd {
        proportion: Decimal,
        recipient: String,
    },
    WithdrawCollateralAndRedeemForUusd {
        proportion: Decimal,
    },
    SendUusdToRecipient {
        proportion: Decimal,
        recipient: String,
    },
    OpenOrIncreaseCdpWithAnchorUstBalanceAsCollateral {
        collateral_ratio: Decimal,
        mirror_asset_cw20_addr: String,
        cdp_idx: Option<Uint128>,
        mirror_asset_mint_amount: Uint128,
    },
    RecordPositionInfo {
        mirror_asset_cw20_addr: String,
    },
    PairUusdWithMirrorAssetToProvideLiquidityAndStake {},
    StakeTerraswapLpTokens {
        lp_token_cw20_addr: String,
    },
    DeltaNeutralReinvest {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControllerExecuteMsg {
    RebalanceAndReinvest {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    OpenPosition {
        params: DeltaNeutralParams,
    },
    IncreasePosition {
        ignore_uusd_pending_unlock: bool,
    },
    DecreasePosition {
        proportion: Decimal,
        recipient: String,
    },
    Controller(ControllerExecuteMsg),
    Internal(InternalExecuteMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // Returns PositionInfoResponse.
    GetPositionInfo {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TerraswapPoolInfo {
    // Amount of LP token staked in Spectrum Mirror farm.
    pub lp_token_amount: Uint128,
    // Address of the LP cw20 token contract.
    pub lp_token_cw20_addr: String,
    // Total supply of the LP token.
    pub lp_token_total_supply: Uint128,
    // Address of the mAsset-UST Terraswap pair contract.
    pub terraswap_pair_addr: String,
    // Balance of mAsset in the mAsset-UST Terraswap pool.
    pub terraswap_pool_mirror_asset_amount: Uint128,
    // Balance of uusd in the mAsset-UST Terraswap pool.
    pub terraswap_pool_uusd_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionState {
    // This contract's uusd balance.
    pub uusd_balance: Uint128,
    // Amount of uusd redeemable from staked LP tokens.
    pub uusd_long_farm: Uint128,
    // Amount of shorted mAsset.
    pub mirror_asset_short_amount: Uint128,
    // This contract's mAsset balance.
    pub mirror_asset_balance: Uint128,
    // Amount of mAsset redeemable from staked LP tokens.
    pub mirror_asset_long_farm: Uint128,
    // Sum of `mirror_asset_balance` and `mirror_asset_long_farm`.
    pub mirror_asset_long_amount: Uint128,
    // Amount of aUST collateral.
    pub collateral_anchor_ust_amount: Uint128,
    // Value of aUST colleteral in uusd.
    pub collateral_uusd_value: Uint128,
    // Address of the mAsset cw20 contract.
    pub mirror_asset_cw20_addr: Addr,
    // Oracle price of the mAsset.
    pub mirror_asset_oracle_price: Decimal,
    // Oracle price of aUST.
    pub anchor_ust_oracle_price: Decimal,
    // Information about the Terraswap mAsset-UST pool.
    // Only populated if long farm is active, i.e. `uusd_long_farm` > 0 and `mirror_asset_long_farm` > 0.
    pub terraswap_pool_info: Option<TerraswapPoolInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TargetCollateralRatioRange {
    pub min: Decimal,
    pub max: Decimal,
}

impl TargetCollateralRatioRange {
    pub fn midpoint(&self) -> Decimal {
        (self.min + self.max) / 2u128.into()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct BlockInfo {
    pub height: u64,
    pub time_nanoseconds: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DetailedPositionInfo {
    pub state: PositionState,
    pub target_collateral_ratio_range: TargetCollateralRatioRange,
    pub collateral_ratio: Decimal,
    pub unclaimed_short_proceeds_uusd_amount: Uint128,
    pub claimable_short_proceeds_uusd_amount: Uint128,
    pub claimable_mir_reward_uusd_value: Uint128,
    pub claimable_spec_reward_uusd_value: Uint128,
    pub uusd_value: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionInfoResponse {
    pub position_open_block_info: BlockInfo,
    pub position_close_block_info: Option<BlockInfo>,
    pub detailed_info: Option<DetailedPositionInfo>,
}
