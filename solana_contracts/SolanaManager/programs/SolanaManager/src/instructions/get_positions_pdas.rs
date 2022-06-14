use anchor_lang::prelude::*;
use crate::state::position::*;

pub fn get_positions_pdas(ctx: Context<GetPositionsPDAs>, user: Pubkey) -> Result<Vec<Pubkey>> {
    let (pda_pubkey, _pda_bump_seed) = Pubkey::find_program_address(&[b"position"], &user.key());
    Ok(vec![pda_pubkey])

}
