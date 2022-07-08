use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account]
pub struct AdminInfo {
  pub admin: Pubkey
}

//TODO add an updateAdmin API