use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;
use crate::state::fee_sink::*;
use std::str::FromStr;

pub fn initialize_admin(ctx: Context<InitializeAdmin>) -> Result<()> {
    let admin_pubkey = Pubkey::from_str("4S6RKWVG9rLiPCj51kGRCbXty7Ht2GVUBuGmkzwqpaCP").unwrap();
    ctx.accounts.admin_info.admin = admin_pubkey;
    Ok(())
}

pub fn update_admin(ctx: Context<UpdateAdmin>, address: Pubkey) -> Result<()> {
    ctx.accounts.admin_info.admin = address;
    Ok(())
}
