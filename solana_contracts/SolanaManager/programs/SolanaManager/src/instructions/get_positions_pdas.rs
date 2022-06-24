use anchor_lang::prelude::*;
use crate::state::position::*;

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