pub mod contract;

mod msg;
mod state;
mod util;

#[cfg(test)]
mod mock_querier;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points_with_migration!(contract);
