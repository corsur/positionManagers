use anchor_lang::prelude::*;
use crate::state::governance::*;

pub fn update_token_whitelist(ctx: Context<UpdateTokenWhitelist>, token_address: Pubkey) -> Result<()> {

    let token_identifier = &mut ctx.accounts.token_identifier;
    token_identifier.token_address = token_address;
    token_identifier.bump = *ctx.bumps.get("token_identifier").unwrap();

    Ok(())

}
