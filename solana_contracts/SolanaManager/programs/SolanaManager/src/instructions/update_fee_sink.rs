use anchor_lang::prelude::*;
use crate::state::feesink::*;

pub fn update_fee_sink(ctx: Context<UpdateFeeSink>, address: Pubkey) -> Result<()> {
    ctx.accounts.fee_sink.fee_sink = address;
    Ok(())
}