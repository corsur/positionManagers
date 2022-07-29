use anchor_lang::prelude::*;
use crate::state::fee_sink::*;

pub fn update_fee_sink(ctx: Context<UpdateFeeSink>, address: Pubkey) -> Result<()> {
    ctx.accounts.fee_sink.fee_sink = address;
    Ok(())
}

pub fn update_cross_chain_fee_bps(ctx: Context<UpdateCrossChainFeeBps>, bps: u32) -> Result<()> {
    ctx.accounts.fee_bps.fee_bps = bps;
    Ok(())
}

pub fn initialize_fee_sink(ctx: Context<InitializeFeeSink>, address: Pubkey) -> Result<()> {
    ctx.accounts.fee_sink.fee_sink = address;
    Ok(())
}