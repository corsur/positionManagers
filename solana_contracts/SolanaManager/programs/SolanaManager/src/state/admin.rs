use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct AdminInfo {
  pub admin: Pubkey
}

#[derive(Accounts)]
pub struct UpdateAdmin<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin)]
    pub admin_info: Account<'info, AdminInfo>
}