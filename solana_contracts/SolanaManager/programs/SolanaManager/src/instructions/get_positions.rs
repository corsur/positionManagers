use anchor_lang::prelude::*;
use crate::state::position::*;

pub fn get_positions(ctx: Context<GetPositions>) -> Result<Vec<PositionInfo>> {

    Ok(vec![ctx.accounts.get_account.position.position_info])

}

#[derive(Accounts)]
pub struct GetPositions<'info> {
    #[account(mut)]
    pub positions: Account<'info, Position>,
    pub get_account: GetPositionsPDAs<'info>
}


