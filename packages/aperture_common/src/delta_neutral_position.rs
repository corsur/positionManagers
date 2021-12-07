use cosmwasm_std::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::delta_neutral_position_manager::DeltaNeutralParams;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InternalExecuteMsg {
    ClaimAndIncreaseUusdBalance {},
    DepositUusdBalanceToAnchor {},
    AddAnchorUstBalanceToCollateral {},
    OpenCdpWithAnchorUstBalanceAsCollateral {
        collateral_ratio: Decimal,
        mirror_asset_cw20_addr: String,
    },
    SwapUusdForMintedMirrorAsset {},
    StakeTerraswapLpTokens {
        lp_token_cw20_addr: String,
        stake_via_spectrum: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControllerExecuteMsg {
    Rebalance {},
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
