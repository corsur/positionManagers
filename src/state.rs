use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read};

static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub anchor_ust_cw20_addr: CanonicalAddr,
    pub mirror_collateral_oracle_addr: CanonicalAddr,
    pub mirror_lock_addr: CanonicalAddr,
    pub mirror_mint_addr: CanonicalAddr,
    pub mirror_oracle_addr: CanonicalAddr,
    pub mirror_staking_addr: CanonicalAddr,
    pub spectrum_mirror_farms_addr: CanonicalAddr,
    pub spectrum_staker_addr: CanonicalAddr,
    pub terraswap_factory_addr: CanonicalAddr,
}

pub fn write_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}
