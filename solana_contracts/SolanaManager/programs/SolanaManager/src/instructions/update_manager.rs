use anchor_lang::prelude::*;
use crate::state::manager::*;

pub fn update_manager(ctx: Context<UpdateManager>, chain: u16, address: Pubkey) -> Result<()> {

    let manager = &mut ctx.accounts.manager;
    manager.chain = chain;
    manager.bump = *ctx.bumps.get("manager").unwrap();
    manager.manager_address = address;
    manager.admin =  ctx.accounts.admin.key();

    Ok(())

}
