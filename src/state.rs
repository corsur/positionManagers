use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read};

static CONFIG_KEY: &[u8] = b"config";
static DELTA_NEUTRAL_INVEST_REQUEST_KEY: &[u8] = b"delta_neutral_invest_request";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
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
}

pub fn write_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeltaNeutralInvestRequest {
    pub collateral_ratio_in_percentage: Uint128,
    pub mirror_asset_cw20_addr: String,
}

pub fn write_delta_neutral_invest_request(
    storage: &mut dyn Storage,
    request: &DeltaNeutralInvestRequest,
) -> StdResult<()> {
    singleton(storage, DELTA_NEUTRAL_INVEST_REQUEST_KEY).save(request)
}

pub fn read_delta_neutral_invest_request(
    storage: &dyn Storage,
) -> StdResult<DeltaNeutralInvestRequest> {
    singleton_read(storage, DELTA_NEUTRAL_INVEST_REQUEST_KEY).load()
}
