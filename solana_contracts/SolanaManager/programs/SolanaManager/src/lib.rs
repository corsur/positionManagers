use anchor_lang::prelude::*;
use instructions::*;
use crate::state::position::*;


declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

pub mod instructions;
pub mod state;

#[program]
pub mod solana_manager {
    use super::*;

    // governance instructions
    // pub fn updateCrossChainFeeBPS(ctx: Context<Wormhole>, bps: u32) -> Result<()> {
    //     instructions::governance::updateCrossChainFeeBPS(ctx, bps)
    // }

    // pub fn updateFeeSink(ctx: Context<Worhmhole>, address: Pubkey) -> Result<()> {
    //     instructions::governance::updateFeeSink(ctx, address)
    // }

    // pub fn updateApertureManager(ctx: Context<Aperture>, chain: u16, address: Pubkey) -> Result<()> {
    //     instructions::governance::updateApertureManager(ctx, chain, address)
    // }

    // pub fn updateTokenWhitelist(ctx: Context<Aperture>, chain: u16, strategy: u64, tokenAddress: Pubkey, whitelisted: bool) -> Result<()> {
    //     instructions::governance::updateTokenWhitelist(ctx, chain, strategy, tokenAddress, whitelisted)
    // }

    // // user instructions
    // pub fn createPosition(ctx: Context<Strategy>, strategyChainId: u16, chain: u16, assetInfo: AssetInfo, encodedPositionOpenData: EncodedPositionOpenData) -> Result<()> {
    //     instructions::user::createPosition(ctx, strategyChainId, chain, assetInfo, encodedPositionOpenData)
    // }

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

    pub fn get_positions(ctx: Context<GetPositions>) -> Result<Vec<PositionInfo>> {
        instructions::get_positions::get_positions(ctx)

    }

    pub fn get_position_pdas(ctx: Context<GetPositionsPDAs>, user: Pubkey) -> Result<Vec<Pubkey>> {
         instructions::get_positions_pdas::get_positions_pdas(ctx, user)
    }
}
