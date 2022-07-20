use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;

#[account]
pub struct ApertureManager {
    pub chain: u16,
    pub manager_address: Pubkey,
    pub bump: u8,
    pub admin: Pubkey
}

#[derive(Accounts)]
#[instruction(chain: u8, address: Pubkey)]
pub struct InitializeManager<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, has_one = admin)]
    pub admin_info: Account<'info, AdminInfo>,
    // space: TBD
    #[account(
        init,
        payer = admin,
        space = 200, seeds = [b"manager", admin.key().as_ref(), &(chain.to_be_bytes())], bump
    )]
    pub manager: Account<'info, ApertureManager>,
    pub system_program: Program<'info, System>,
}

impl ApertureManager {

    pub fn get_position_pubkey(manager: Pubkey) -> Result<Pubkey> {
        let (pda_pubkey, _pda_bump_seed) = Pubkey::find_program_address(&[b"manager"], &manager.key());
        Ok(pda_pubkey)
    }

}