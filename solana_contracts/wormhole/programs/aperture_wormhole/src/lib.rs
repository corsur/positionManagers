use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::borsh::try_from_slice_unchecked;
use anchor_lang::solana_program::program::invoke_signed;


use sha3::Digest;
use byteorder::{
    BigEndian,
    WriteBytesExt,
};
use std::io::{
    Cursor,
    Write,
};
use std::str::FromStr;
use hex::decode;

mod context;
mod constant;
mod account;
mod wormhole;
mod error;

use wormhole::*;
use context::*;
use constant::*;
use error::*;

declare_id!("FVVVy4Uvdax5rjR4tiKq89TkgF8pByZfS7Xof6Z5bHQ2");

#[program]
pub mod aperture_wormhole {

    use super::*;
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.config.owner = ctx.accounts.owner.key();
        ctx.accounts.config.nonce = 1;
        Ok(())
    }

    pub fn register_chain(ctx:Context<RegisterChain>, chain_id:u16, emitter_addr:String) -> Result<()> {
        ctx.accounts.emitter_acc.chain_id = chain_id;
        ctx.accounts.emitter_acc.emitter_addr = emitter_addr;
        Ok(())
    }

    pub fn confirm_msg(ctx:Context<ConfirmMsg>) -> Result<()> {
        //Hash a VAA Extract and derive a VAA Key
        let vaa = PostedMessageData::try_from_slice(&ctx.accounts.core_bridge_vaa.data.borrow())?.0;
        let serialized_vaa = serialize_vaa(&vaa);

        let mut h = sha3::Keccak256::default();
        h.write(serialized_vaa.as_slice()).unwrap();
        let vaa_hash: [u8; 32] = h.finalize().into();

        let (vaa_key, _) = Pubkey::find_program_address(&[
            b"PostedVAA",
            &vaa_hash
        ], &Pubkey::from_str(CORE_BRIDGE_ADDRESS).unwrap());

        if ctx.accounts.core_bridge_vaa.key() != vaa_key {
            return err!(MessengerError::VAAKeyMismatch);
        }

        msg!("Checking emitter chain and address");
        // Already checked that the SignedVaa is owned by core bridge in account constraint logic
        //Check that the emitter chain and address match up with the vaa
        if vaa.emitter_chain != ctx.accounts.emitter_acc.chain_id ||
           vaa.emitter_address != &decode(&ctx.accounts.emitter_acc.emitter_addr.as_str()).unwrap()[..] {
            return err!(MessengerError::VAAEmitterMismatch)
        }

        ctx.accounts.config.current_msg = String::from_utf8(vaa.payload).unwrap();
        Ok(())
    }

    pub fn publish_execute_strategy_instruction(
        ctx: Context<PublishExecuteStrategyInstruction>,
        msg: String,
    ) -> Result<()> {
            //Look Up Fee
            let bridge_data:BridgeData = try_from_slice_unchecked(&ctx.accounts.wormhole_config.data.borrow_mut())?;
    
            //Send Fee
            invoke_signed(
                &transfer(
                    &ctx.accounts.payer.key(),
                    &ctx.accounts.wormhole_fee_collector.key(),
                    bridge_data.config.fee
                ),
                &[
                    ctx.accounts.payer.to_account_info(),
                    ctx.accounts.wormhole_fee_collector.to_account_info()
                ],
                &[]
            )?;
    
            //Send Post Msg Tx
            let sendmsg_ix = Instruction {
                program_id: ctx.accounts.core_bridge.key(),
                accounts: vec![
                    AccountMeta::new(ctx.accounts.wormhole_config.key(), false),
                    AccountMeta::new(ctx.accounts.wormhole_message_key.key(), true),
                    AccountMeta::new_readonly(ctx.accounts.wormhole_derived_emitter.key(), true),
                    AccountMeta::new(ctx.accounts.wormhole_sequence.key(), false),
                    AccountMeta::new(ctx.accounts.payer.key(), true),
                    AccountMeta::new(ctx.accounts.wormhole_fee_collector.key(), false),
                    AccountMeta::new_readonly(ctx.accounts.clock.key(), false),
                    AccountMeta::new_readonly(ctx.accounts.rent.key(), false),
                    AccountMeta::new_readonly(ctx.accounts.system_program.key(), false),
                ],
                data: (
                    wormhole::Instruction::PostMessage,
                    PostMessageData {
                        nonce: ctx.accounts.config.nonce,
                        payload: msg.as_bytes().try_to_vec()?,
                        consistency_level: wormhole::ConsistencyLevel::Confirmed,
                    },
                ).try_to_vec()?,
            };
    
            invoke_signed(
                &sendmsg_ix,
                &[
                    ctx.accounts.wormhole_config.to_account_info(),
                    ctx.accounts.wormhole_message_key.to_account_info(),
                    ctx.accounts.wormhole_derived_emitter.to_account_info(),
                    ctx.accounts.wormhole_sequence.to_account_info(),
                    ctx.accounts.payer.to_account_info(),
                    ctx.accounts.wormhole_fee_collector.to_account_info(),
                    ctx.accounts.clock.to_account_info(),
                    ctx.accounts.rent.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
                &[
                    &[
                        &b"emitter".as_ref(),
                        &[*ctx.bumps.get("wormhole_derived_emitter").unwrap()]
                    ]
                ]
            )?;
    
            ctx.accounts.config.nonce += 1;
            Ok(())
    }

    pub fn expand_asset_infos(
        ctx: Context<ExpandAssetInfos>,
        address: Pubkey,
    ) -> Result<()> {
        let asset = &mut ctx.accounts.asset;
        asset.bump = *ctx.bumps.get("assetinfo").unwrap();
        asset.asset_address = address;
        Ok(())
    }

    pub fn initialize_wormhole_settings(ctx: Context<InitializeWormholeContext>) -> Result<()> {
        let wormhole_settings = &mut ctx.accounts.wormhole_settings;
        wormhole_settings.bump = *ctx.bumps.get("wormholesettings").unwrap();
        let inferred_wormhole_settings = &mut ctx.accounts.inferred_wormhole_settings;
        inferred_wormhole_settings.bump = *ctx.bumps.get("inferredwormholesettings").unwrap();
        let fee_settings = &mut ctx.accounts.fee_settings;
        fee_settings.bump = *ctx.bumps.get("crosschainfeesettings").unwrap();
        Ok(())
    }

    pub fn update_wormhole_settings(ctx: Context<UpdateWormholeContext>) -> Result<()> {
        let wormhole_settings = &mut ctx.accounts.wormhole_settings;
        wormhole_settings.bump = *ctx.bumps.get("wormholesettings").unwrap();
        let inferred_wormhole_settings = &mut ctx.accounts.inferred_wormhole_settings;
        inferred_wormhole_settings.bump = *ctx.bumps.get("inferredwormholesettings").unwrap();
        let fee_settings = &mut ctx.accounts.fee_settings;
        fee_settings.bump = *ctx.bumps.get("crosschainfeesettings").unwrap();
        Ok(())
    }
}

// Convert a full VAA structure into the serialization of its unique components, this structure is
// what is hashed and verified by Guardians.
pub fn serialize_vaa(vaa: &MessageData) -> Vec<u8> {
    let mut v = Cursor::new(Vec::new());
    v.write_u32::<BigEndian>(vaa.vaa_time).unwrap();
    v.write_u32::<BigEndian>(vaa.nonce).unwrap();
    v.write_u16::<BigEndian>(vaa.emitter_chain.clone() as u16).unwrap();
    v.write(&vaa.emitter_address).unwrap();
    v.write_u64::<BigEndian>(vaa.sequence).unwrap();
    v.write_u8(vaa.consistency_level).unwrap();
    v.write(&vaa.payload).unwrap();
    v.into_inner()
}