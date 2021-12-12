use cosmwasm_std::{Decimal, Uint128};
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
    WithdrawUusd {
        proportion: Decimal,
        recipient: String,
    },
    DepositUusdBalanceToAnchor {},
    AddAnchorUstBalanceToCollateral {},
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
    IncreasePosition {},
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
    GetPositionInfo {},
}
