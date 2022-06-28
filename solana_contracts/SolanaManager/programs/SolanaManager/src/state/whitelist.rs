use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct TokenIdentifier {
    pub chain: u16,
    pub strategy: u64,
    pub token_address: Pubkey,
    pub whitelisted: bool,
    pub bump: u8
}

#[derive(Accounts)]
#[instruction(token_address: Pubkey)]
pub struct UpdateTokenWhitelist<'info> {
    #[account(mut)]
    pub solana_manager: Signer<'info>,
    // space: TBD
    #[account(
        init,
        payer = solana_manager,
        space = 8 + 2 + 4 + 200 + 1, seeds = [b"token_identifier", solana_manager.key().as_ref()], bump
    )]
    pub token_identifier: Account<'info, TokenIdentifier>,
    pub system_program: Program<'info, System>,
}