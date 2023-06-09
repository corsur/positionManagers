use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq, 
)]
pub struct PositionInfo {
    position_id: u128, // The position id.
    chain_id: u16 // Chain id, following Wormhole's design.
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq,
)]
pub struct StoredPositionInfo {
    owner_addr: Pubkey,
    strategy_chain_id: u16,
    strategy_id: u64
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq,
)]
pub struct AssetInfo {
    asset_addr: Pubkey, // The token address.
    amount: u128
}

#[account]
pub struct Position {
    pub stored_position_info: StoredPositionInfo,
    pub position_info: PositionInfo,
    pub asset_info: AssetInfo,
    pub bump: u8,
    pub extra_seed: u8
}

#[derive(Accounts)]
#[instruction(extra_seed: u8)]
pub struct CreatePosition<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    // space: TBD
    #[account(
        init,
        payer = user,
        space = 8 + 2 + 4 + 200 + 1, seeds = [b"position", user.key().as_ref(), &[extra_seed as u8]], bump
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
pub struct GetPositionsPDAs<'info> {
    pub user: Signer<'info>,
    #[account(mut, seeds = [b"position", user.key().as_ref()], bump = position.bump)]
    pub position: Account<'info, Position>,
}

impl Position {

    pub fn get_position_pubkey(user: Pubkey) -> Result<Pubkey> {
        let (pda_pubkey, _pda_bump_seed) = Pubkey::find_program_address(&[b"position"], &user.key());
        Ok(pda_pubkey)
    }

    pub fn create_position(ctx: Context<CreatePosition>, address: Pubkey) -> Result<()> {
        let position = &mut ctx.accounts.position;
        position.stored_position_info.owner_addr = address;
        position.stored_position_info.strategy_chain_id = 1;
        position.stored_position_info.strategy_id = 1;
        position.position_info.position_id = 1;
        position.position_info.chain_id = 1;

        position.bump = *ctx.bumps.get("position").unwrap();
        Ok(())
    }
}