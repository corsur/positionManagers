use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map, U32Key};

pub const OWNER: Item<Addr> = Item::new("owner");
pub const STRATEGIES: Map<U32Key, Addr> = Map::new("strategies");
