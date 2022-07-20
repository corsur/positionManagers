use anchor_lang::prelude::*;
use instructions::*;
use crate::state::position::*;
use crate::state::manager::*;
use crate::state::token_whitelist::*;
use crate::state::fee_sink::*;
use crate::state::admin::*;
use crate::state::strategy::*;

//devnet
//declare_id!("7ySNekmtGq9NMjnWGb7YHtTGio7AcV4665cp9MwV3rVe");
//localnet
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

pub mod instructions;
pub mod state;

#[program]
pub mod solana_manager {
    use super::*;

    // governance instructions

    pub fn initialize_admin(ctx: Context<InitializeAdmin>) -> Result<()> {
        instructions::admin::initialize_admin(ctx)
    }

    pub fn update_admin(ctx: Context<UpdateAdmin>, address: Pubkey) -> Result<()> {
        instructions::admin::update_admin(ctx, address)
    }

    pub fn update_cross_chain_fee_bps(ctx: Context<UpdateCrossChainFeeBps>, bps: u32) -> Result<()> {
        instructions::fee_sink::update_cross_chain_fee_bps(ctx, bps)
    }

    pub fn initialize_fee_sink(ctx: Context<InitializeFeeSink>, address: Pubkey) -> Result<()> {
        instructions::fee_sink::initialize_fee_sink(ctx, address)
    }

    pub fn update_fee_sink(ctx: Context<UpdateFeeSink>, address: Pubkey) -> Result<()> {
        instructions::fee_sink::update_fee_sink(ctx, address)
    }

    pub fn initialize_manager(ctx: Context<InitializeManager>, chain: u16, address: Pubkey) -> Result<()> {
        instructions::manager::initialize_manager(ctx, chain, address)
    }

    pub fn update_token_whitelist(ctx: Context<UpdateTokenWhitelist>, token_address: Pubkey) -> Result<()> {
        instructions::token_whitelist::update_token_whitelist(ctx, token_address)
    }

    // user instructions
    
    pub fn create_position(ctx: Context<CreatePosition>, extra_seed: u8) -> Result<()> {
        instructions::position::create_position(ctx, extra_seed)
    }

    pub fn get_positions(ctx: Context<GetPositions>) -> Result<Vec<PositionInfo>> {
        instructions::position::get_positions(ctx)
    }

    pub fn get_position_pdas(ctx: Context<GetPositionsPDAs>, user: Pubkey) -> Result<VectorPubkey> {
         instructions::position::get_positions_pdas(ctx, user)
    }

    pub fn execute_strategy(ctx: Context<ExecuteStrategy>, stored_position_info: StoredPositionInfo, position_info: PositionInfo, asset_info: AssetInfo, action: Action) -> Result<()> {
        instructions::strategy::execute_strategy(ctx, stored_position_info, position_info, asset_info, action)
    }
}
