use anchor_lang::prelude::*;
use crate::state::position::*;

pub fn create_position(ctx: Context<CreatePosition>, extra_seed: u8) -> Result<()> {

    let position = &mut ctx.accounts.position;
    position.extra_seed = extra_seed;
    position.bump = *ctx.bumps.get("position").unwrap();

    Ok(())

}
