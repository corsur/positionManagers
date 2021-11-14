use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, StdResult, Storage, StdError};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

static CONFIG_KEY: &[u8] = b"config";
static INVESTMENT_REGISTRY_KEY: &[u8] = b"investment_registry";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
}

pub fn write_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}

pub fn write_investment_registry(
    storage: &mut dyn Storage,
    strategy_index: u64,
    strategy_manager_addr: &Addr,
) -> StdResult<()> {
    let mut bucket: Bucket<Addr> = Bucket::new(storage, INVESTMENT_REGISTRY_KEY);
    bucket.save(&strategy_index.to_be_bytes(), strategy_manager_addr)
}

pub fn read_investment_registry(
    storage: &dyn Storage,
    strategy_index: u64,
) -> StdResult<Addr> {
    let bucket: ReadonlyBucket<Addr> =
        ReadonlyBucket::new(storage, INVESTMENT_REGISTRY_KEY);
    let res = bucket.load(&strategy_index.to_be_bytes());
    match res {
        Ok(data) => Ok(data),
        _ => Err(StdError::generic_err("no associated investment stored")),
    }
}
