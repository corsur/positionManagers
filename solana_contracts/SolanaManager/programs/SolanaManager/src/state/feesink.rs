use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;

#[account]
pub struct FeeSink {
  pub fee_sink: Pubkey
}

#[derive(Accounts)]
pub struct UpdateFeeSink<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin)]
    pub admin_info: Account<'info, AdminInfo>,
    #[account(mut)]
    pub fee_sink: Account<'info, FeeSink>
}

#[account]
pub struct FeeBps {
  pub fee_bps: u32
}

#[derive(Accounts)]
pub struct UpdateCrossChainFeeBps<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin)]
    pub admin_info: Account<'info, AdminInfo>,
    #[account(mut)]
    pub fee_bps: Account<'info, FeeBps>
}