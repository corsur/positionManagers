use crate::state::position::*;
use anchor_lang::prelude::*;

pub fn get_positions(ctx: Context<GetPositions>, address: Pubkey) -> Result<PositionInfo> {
    ctx.accounts
        .positions
        .get_positions(address)
}

#[derive(Accounts)]
pub struct GetPositions<'info> {
    #[account(mut)]
    pub positions: Account<'info, Position>
}
