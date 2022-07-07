use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct TokenIdentifier {
    pub chain: u16,
    pub strategy: u64,
    pub token_address: Pubkey,
    pub whitelisted: bool,
    pub bump: u8,
    pub admin: Pubkey
}

#[derive(Accounts)]
#[instruction(token_address: Pubkey)]
pub struct UpdateTokenWhitelist<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    // space: TBD
    #[account(
        init,
        has_one = admin,
        payer = admin,
        space = 8 + 2 + 4 + 200 + 1, seeds = [b"token_identifier", admin.key().as_ref()], bump
    )]
    pub token_identifier: Account<'info, TokenIdentifier>,
    pub system_program: Program<'info, System>,
}