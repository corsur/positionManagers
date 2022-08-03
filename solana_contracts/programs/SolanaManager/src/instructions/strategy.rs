use anchor_lang::prelude::*;
use crate::state::strategy::*;
use crate::state::position::*;

pub fn execute_strategy(
    ctx: Context<ExecuteStrategy>, stored_position_info: StoredPositionInfo, 
    position_info: PositionInfo, asset_info: AssetInfo, action: Action) -> Result<()> {
    Ok(())
}