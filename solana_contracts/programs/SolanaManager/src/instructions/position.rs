use anchor_lang::prelude::*;
use crate::state::position::*;

pub fn create_position(ctx: Context<CreatePosition>, extra_seed: u8) -> Result<()> {

    let position = &mut ctx.accounts.position;
    position.extra_seed = extra_seed;
    position.bump = *ctx.bumps.get("position").unwrap();

    Ok(())

}

pub fn get_positions(ctx: Context<GetPositions>) -> Result<Vec<PositionInfo>> {

    Ok(vec![ctx.accounts.get_account.position.position_info])

}

#[derive(Accounts)]
pub struct GetPositions<'info> {
    #[account(mut)]
    pub positions: Account<'info, Position>,
    pub get_account: GetPositionsPDAs<'info>
}

pub fn get_positions_pdas(ctx: Context<GetPositionsPDAs>, user: Pubkey) -> Result<VectorPubkey> {
    let (pda_pubkey, _pda_bump_seed) = Pubkey::find_program_address(&[b"position"], &user.key());
    let PDAs = VectorPubkey { vector: vec![pda_pubkey] };
    Ok(PDAs)

}

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq,
)]
pub struct VectorPubkey {
    pub vector: Vec<Pubkey>
}
