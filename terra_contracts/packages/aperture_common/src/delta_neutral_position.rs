use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{common::Recipient, delta_neutral_position_manager::DeltaNeutralParams};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

/// Internal message to achieve better logic flow.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    AchieveSafeCollateralRatio {},
    CloseCdpAndDisburseUusd {
        recipient: Recipient,
    },
    CloseCdpAndDepositToAnchorEarn {},
    DepositUusdBalanceToAnchorEarn {},
    WithdrawCollateralAndRedeemForUusd {
        proportion: Decimal,
    },
    SendUusdToRecipient {
        proportion: Decimal,
        recipient: Recipient,
    },
    PairUusdWithMirrorAssetToProvideLiquidityAndStake {},
    DeltaNeutralReinvest {
        mirror_asset_fresh_oracle_uusd_rate: Decimal,
    },
    // Performs a sanity check at the end of ExecuteMsg::OpenPosition to make sure that the short and long positions hold an equal amount of the mAsset.
    OpenPositionSanityCheck {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControllerExecuteMsg {
    RebalanceAndReinvest {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    OpenPosition { params: DeltaNeutralParams },
    ClosePosition { recipient: Recipient },
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
    // Returns bool.
    CheckSpectrumMirrorFarmExistence { mirror_asset_cw20_addr: String },
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
    // The number of auto-compound shares in the Spectrum Mirror Farm.
    // These shares can currently be redeemed for `lp_token_amount` amount of LP tokens.
    // This value is represented as `auto_bond_share` in Spectrum Mirror Farm reward info.
    pub spectrum_auto_compound_share_amount: Uint128,
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
    // Oracle price of the mAsset.
    pub mirror_asset_oracle_price: Decimal,
    // Oracle price of aUST.
    pub anchor_ust_oracle_price: Decimal,
    // Information about the Terraswap mAsset-UST pool.
    pub terraswap_pool_info: TerraswapPoolInfo,
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
pub struct PositionActionInfo {
    pub height: u64,
    pub time_nanoseconds: u64,
    pub uusd_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DetailedPositionInfo {
    pub cdp_preemptively_closed: bool,
    // None if either:
    // (1) the position was opened when oracle price was stale and the position is currently pending DN setup; OR
    // (2) the CDP has been preemptively closed and the funds are currently in Anchor Earn.
    pub state: Option<PositionState>,
    pub target_collateral_ratio_range: TargetCollateralRatioRange,
    // None if either:
    // (1) the position was opened when oracle price was stale and the position is currently pending DN setup; OR
    // (2) the CDP has been preemptively closed and the funds are currently in Anchor Earn.
    pub collateral_ratio: Option<Decimal>,
    pub unclaimed_short_proceeds_uusd_amount: Uint128,
    pub claimable_short_proceeds_uusd_amount: Uint128,
    pub claimable_mir_reward_uusd_value: Uint128,
    pub claimable_spec_reward_uusd_value: Uint128,
    pub uusd_value: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PositionInfoResponse {
    pub position_open_info: PositionActionInfo,
    // None if position is closed.
    pub position_close_info: Option<PositionActionInfo>,
    // None if position was opened when oracle price was stale and the position is currently pending DN setup.
    pub cdp_idx: Option<Uint128>,
    pub mirror_asset_cw20_addr: Addr,
    // None if position is closed.
    pub detailed_info: Option<DetailedPositionInfo>,
}
