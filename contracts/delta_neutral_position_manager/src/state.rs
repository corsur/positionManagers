use aperture_common::common::DeltaNeutralParams;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_binary, Addr, StdError, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

static CONFIG_KEY: &[u8] = b"config";
static CONTRACT_KEY: &[u8] = b"contract";
static PARAMS_KEY: &[u8] = b"params";

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

pub fn write_params(storage: &mut dyn Storage, params: &DeltaNeutralParams) -> StdResult<()> {
    singleton(storage, PARAMS_KEY).save(params)
}

pub fn read_params(storage: &dyn Storage) -> StdResult<DeltaNeutralParams> {
    singleton_read(storage, PARAMS_KEY).load()
}

pub fn write_contract_registry(
    storage: &mut dyn Storage,
    position_id: Uint128,
    strategy_addr: &Addr,
) -> StdResult<()> {
    let mut bucket: Bucket<Addr> = Bucket::new(storage, CONTRACT_KEY);
    bucket.save(
        to_binary(&position_id)?.as_slice(),
        strategy_addr,
    )
}

/// Get associated contract address for the position id.
///
/// Arguments:
///
/// * `position_id`: the unique identifier representing each strategy.
pub fn read_contract_registry(
    storage: &dyn Storage,
    position_id: Uint128,
) -> StdResult<Addr> {
    let bucket: ReadonlyBucket<Addr> = ReadonlyBucket::new(storage, CONTRACT_KEY);
    let res = bucket.load(to_binary(&position_id)?.as_slice());
    match res {
        Ok(data) => Ok(data),
        _ => Err(StdError::generic_err("No associated contract stored")),
    }
}
