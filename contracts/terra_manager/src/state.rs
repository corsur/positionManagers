use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_binary, Addr, StdError, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

use aperture_common::common::StrategyType;

static CONFIG_KEY: &[u8] = b"config";
static INVESTMENT_REGISTRY_KEY: &[u8] = b"investment_registry";

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

/// Write pair <strategy_index, strategy_manager_addr> into a map (represented)
/// as a Bucket.
///
/// Arguments:
///
/// * `strategy_index`: the unique identifier representing each strategy.
/// * `strategy_manager_addr`: the contract address for the underlying strategy
///   manager.
pub fn write_investment_registry(
    storage: &mut dyn Storage,
    strategy_index: StrategyType,
    strategy_manager_addr: &Addr,
) -> StdResult<()> {
    let mut bucket: Bucket<Addr> = Bucket::new(storage, INVESTMENT_REGISTRY_KEY);
    bucket.save(
        to_binary(&strategy_index)?.as_slice(),
        strategy_manager_addr,
    )
}

/// Get associated investment strategy contract address for the strategy index.
///
/// Arguments:
///
/// * `strategy_index`: the unique identifier representing each strategy.
pub fn read_investment_registry(
    storage: &dyn Storage,
    strategy_index: StrategyType,
) -> StdResult<Addr> {
    let bucket: ReadonlyBucket<Addr> = ReadonlyBucket::new(storage, INVESTMENT_REGISTRY_KEY);
    let res = bucket.load(to_binary(&strategy_index)?.as_slice());
    match res {
        Ok(data) => Ok(data),
        _ => Err(StdError::generic_err("No associated investment stored")),
    }
}
