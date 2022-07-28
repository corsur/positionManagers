use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;

#[account]
pub struct FeeSink {
  pub fee_sink: Pubkey,
  pub bump: u8,
}

#[derive(Accounts)]
pub struct UpdateFeeSink<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin, seeds = [b"admininfo"], bump = admin_info.bump)]
    pub admin_info: Account<'info, AdminInfo>,
    #[account(mut, seeds = [b"feesink"], bump)]
    pub fee_sink: Account<'info, FeeSink>
}

#[derive(Accounts)]
pub struct InitializeFeeSink<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin, seeds = [b"admininfo"], bump = admin_info.bump)]
    pub admin_info: Account<'info, AdminInfo>,
    // space = 8 for discriminator, 32 for pubkey, 1 for bump
    #[account(init, payer = admin, space = 41, seeds = [b"feesink"], bump)]
    pub fee_sink: Account<'info, FeeSink>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct FeeBps {
  pub fee_bps: u32
}

#[derive(Accounts)]
pub struct UpdateCrossChainFeeBps<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin, seeds = [b"admininfo"], bump = admin_info.bump)]
    pub admin_info: Account<'info, AdminInfo>,
    #[account(mut)]
    pub fee_bps: Account<'info, FeeBps>
}
