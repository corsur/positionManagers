use anchor_lang::prelude::*;
use crate::state::feesink::*;

pub fn update_cross_chain_fee_bps(ctx: Context<UpdateCrossChainFeeBps>, bps: u32) -> Result<()> {
    ctx.accounts.fee_bps.fee_bps = bps;
    Ok(())
}