use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;
use crate::state::position::*;

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq,
)]
pub enum Action {
    OpenPosition {
        data: u8,
    },
}

#[derive(Accounts)]
pub struct ExecuteStrategy<'info> {
    #[account(mut)]
    pub owner_addr: Signer<'info>,
}
