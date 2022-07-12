use anchor_lang::prelude::*;
use crate::state::admin::*;

pub fn update_admin(ctx: Context<UpdateAdmin>, address: Pubkey) -> Result<()> {
    ctx.accounts.admin_info.admin = address;
    Ok(())
}