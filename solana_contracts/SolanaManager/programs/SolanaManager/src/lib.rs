use anchor_lang::prelude::*;
use instructions::*;
use crate::state::position::*;
use crate::state::manager::*;
use crate::state::whitelist::*;
use crate::state::feesink::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

pub mod instructions;
pub mod state;

#[program]
pub mod solana_manager {
    use super::*;

    // // user instructions

    // pub fn swapTokenAndCreatePosition(
    //     ctx: Context<Strategy>, fromToken: Pubkey, toToken: Pubkey, amount: u256, minAmountOut: u256, 
    //     strategy: u64, strategyChainId: u16, encodedPositionOpenData: EncodedPositionOpenData) -> Result<()> {
    //     instructions::user::swapTokenAndCreatePosition(ctx, fromToken, toToken, amount, minAmountOUt, strategy, strategyChainId, encodedPositionOpenData)
    // }

    // pub fn executeStrategy(ctx: Context<Strategy>, positionId: u128, assetInfo: AssetInfo, encodedPositionOpenData: EncodedPositionOpenData) -> Result<()> {
    //     instructions::user::executeStrategy(ctx, strategyChainId, chain, assetInfo, encodedPositionOpenData)
    // }

    // pub fn swapTokenAndExecuteStrategy(
    //     ctx: Context<Strategy>, fromToken: Pubkey, toToken: Pubkey, amount: u256, minAmountOut: u256, 
    //     positionId: u128, encodedPositionOpenData: EncodedPositionOpenData) -> Result<()> {
    //     instructions::user::swapTokenAndExecuteStrategy(ctx, fromToken, toToken, amount, minAmountOUt, positionId, encodedPositionOpenData)
    // }

    // governance instructions

    pub fn update_cross_chain_fee_bps(ctx: Context<UpdateCrossChainFeeBps>, bps: u32) -> Result<()> {
        instructions::update_cross_chain_fee_bps::update_cross_chain_fee_bps(ctx, bps)
    }

    pub fn update_fee_sink(ctx: Context<UpdateFeeSink>, address: Pubkey) -> Result<()> {
        instructions::update_fee_sink::update_fee_sink(ctx, address)
    }

    pub fn update_manager(ctx: Context<UpdateManager>, chain: u16, address: Pubkey) -> Result<()> {
        instructions::update_manager::update_manager(ctx, chain, address)
    }

    pub fn update_token_whitelist(ctx: Context<UpdateTokenWhitelist>, token_address: Pubkey) -> Result<()> {
        instructions::update_token_whitelist::update_token_whitelist(ctx, token_address)
    }

    // user instructions
    
    pub fn create_position(ctx: Context<CreatePosition>, extra_seed: u8) -> Result<()> {
        instructions::create_position::create_position(ctx, extra_seed)
    }

    pub fn get_positions(ctx: Context<GetPositions>) -> Result<Vec<PositionInfo>> {
        instructions::get_positions::get_positions(ctx)
    }

    pub fn get_position_pdas(ctx: Context<GetPositionsPDAs>, user: Pubkey) -> Result<VectorPubkey> {
         instructions::get_positions_pdas::get_positions_pdas(ctx, user)
    }
}
