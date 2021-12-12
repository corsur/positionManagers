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
    GetPositionState {},
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
    // Amount of LP token staked in Spectrum Mirror farm.
    pub lp_token_amount: Uint128,
    // Address of the LP cw20 token contract.
    pub lp_token_cw20_addr: String,
    // Address of the mAsset-UST Terraswap pair contract.
    pub terraswap_pair_addr: String,
}
