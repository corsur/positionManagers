use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Structures commonly shared by Aperture contracts.

/// Action enum that represents what users can do to each strategy.
/// For instance, users can open a position, which is represented by the
/// OpenPosition variant.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyAction {
    OpenPosition {},
    ClosePosition {},
    IncreasePosition {},
    DecreasePosition {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct DeltaNeutralParams {
    pub collateral_ratio_in_percentage: Uint128,
    pub mirror_asset_cw20_addr: String,
    pub position_id: Uint128,
}

/// Enum represents individual strategy. When new strategies are added,
/// they must be added here to be used by smart contracts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum StrategyType {
    DeltaNeutral(DeltaNeutralParams),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub native: bool,
    pub denom: String,
    pub addr: Addr,
    pub amount: Uint128,
}
