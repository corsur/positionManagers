use anchor_lang::prelude::*;
use crate::state::manager::*;

pub fn initialize_manager(ctx: Context<InitializeManager>, chain: u16, address: Pubkey) -> Result<()> {

    let manager = &mut ctx.accounts.manager;
    manager.chain = chain;
    manager.bump = *ctx.bumps.get("manager").unwrap();
    manager.manager_address = address;

    Ok(())

}
