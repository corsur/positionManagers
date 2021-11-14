use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, StdResult, Storage, StdError};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

static CONFIG_KEY: &[u8] = b"config";

/// Basic config to be stored in storage.
/// * `owner`: the owner of this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
}

/// Persist config into storage.
/// 
/// Arguments:
/// 
/// * `storage`: the mutable storage to write into.
/// * `config`: the config struct to be stored.
pub fn write_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

/// Read-only method to examine the content of config.
/// 
/// Arguments:
/// 
/// * `storage`: the read-only storage to get config from.
pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}