use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct AdminInfo {
  pub admin: Pubkey,
  pub bump: u8
}

#[derive(Accounts)]
pub struct UpdateAdmin<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin, seeds = [b"admininfo"], bump = admin_info.bump)]
    pub admin_info: Account<'info, AdminInfo>
}

#[derive(Accounts)]
pub struct InitializeAdmin<'info> {
    #[account(init, payer = initializer, space = 41, seeds = [b"admininfo"], bump)]
    pub admin_info: Account<'info, AdminInfo>,
    #[account(mut)]
    pub initializer: Signer<'info>,
    pub system_program: Program<'info, System>,
}