use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use crate::state::admin::*;
use crate::state::fee_sink::*;
use std::str::FromStr;

pub fn initialize_admin(ctx: Context<InitializeAdmin>) -> Result<()> {
    let admin_info = &mut ctx.accounts.admin_info;
    admin_info.admin = Pubkey::from_str("4S6RKWVG9rLiPCj51kGRCbXty7Ht2GVUBuGmkzwqpaCP").unwrap();
    admin_info.bump = *ctx.bumps.get("admin_info").unwrap();
    Ok(())
}

pub fn update_admin(ctx: Context<UpdateAdmin>, address: Pubkey) -> Result<()> {
    ctx.accounts.admin_info.admin = address;
    Ok(())
}
