use anchor_lang::prelude::*;
use crate::state::position::*;

pub fn create_position(ctx: Context<CreatePosition>) -> Result<()> {

    let position = &mut ctx.accounts.position;

    position.bump = *ctx.bumps.get("position").unwrap();
    Ok(())

}
