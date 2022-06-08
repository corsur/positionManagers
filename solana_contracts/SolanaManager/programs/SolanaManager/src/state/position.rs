use anchor_lang::prelude::*;

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq, 
)]
pub struct PositionInfo {
    position_id: u128, // The position id.
    chain_id: u16 // Chain id, following Wormhole's design.
}

#[account]
pub struct StoredPositionInfo {
    owner_addr: Pubkey,
    strategy_cchain_id: u16,
    strategy_id: u64
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq,
)]
pub struct AssetInfo {
    asset_addr: Pubkey, // The token address.
    amount: u128
}

#[account]
pub struct Position {
    stored_position_info: StoredPositionInfo,
    position_info: PositionInfo
}

impl Position {
    pub fn get_positions(&mut self, address: Pubkey) -> Result<PositionInfo> {
        let mut example_position = PositionInfo {position_id: 0, chain_id: 0};
        Ok(example_position)
    }
}