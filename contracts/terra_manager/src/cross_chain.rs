use aperture_common::{
    byte_util::{extend_terra_address_to_32, ByteUtils},
    common::{Action, ChainId, Position, Recipient, StrategyId},
    terra_manager::TERRA_CHAIN_ID,
    token_util::{forward_assets_direct, validate_and_accept_incoming_asset_transfer},
    wormhole::{
        ParsedVAA, TokenBridgeMessage, TransferInfo, WormholeCoreBridgeQueryMsg,
        WormholeTokenBridgeExecuteMsg,
    },
};
use cosmwasm_std::{
    entry_point, from_binary, to_binary, BankMsg, Binary, Coin, ContractResult, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw_storage_plus::U16Key;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    state::{
        ADMIN, CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP, COMPLETED_INSTRUCTIONS,
        CROSS_CHAIN_OUTGOING_FEE_CONFIG, WORMHOLE_CORE_BRIDGE_ADDR, WORMHOLE_TOKEN_BRIDGE_ADDR,
    },
    terra_chain::{create_execute_strategy_messages, save_new_position_info_and_open_it},
};

static TOKEN_TRANSFER_SUBMIT_VAA_MSG_ID: u64 = 0;

pub fn initiate_outgoing_token_transfer(
    deps: Deps,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    recipient: Recipient,
) -> StdResult<Response> {
    let mut response = Response::new().add_messages(validate_and_accept_incoming_asset_transfer(
        env, info, &assets,
    )?);
    match recipient {
        Recipient::TerraChain { recipient } => {
            let (funds, cw20_transfer_messages) =
                forward_assets_direct(&assets, &deps.api.addr_validate(&recipient)?)?;
            response = response.add_messages(cw20_transfer_messages);
            if !funds.is_empty() {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: recipient,
                    amount: funds,
                }))
            }
        }
        Recipient::ExternalChain {
            recipient_chain,
            recipient,
        } => {
            let cross_chain_outgoing_fee_config =
                CROSS_CHAIN_OUTGOING_FEE_CONFIG.load(deps.storage)?;
            let wormhole_token_bridge_addr = WORMHOLE_TOKEN_BRIDGE_ADDR.load(deps.storage)?;

            // Calculate asset amount to be sent to fee collector address, and amount to be transferred cross-chain to `recipient`.
            let mut fee_collection_assets = vec![];
            let mut cross_chain_assets = vec![];
            for asset in assets.iter() {
                let fee_amount = asset.amount * cross_chain_outgoing_fee_config.rate;
                let cross_chain_amount = asset.amount - fee_amount;
                if !fee_amount.is_zero() {
                    fee_collection_assets.push(Asset {
                        amount: fee_amount,
                        info: asset.info.clone(),
                    });
                }
                if !cross_chain_amount.is_zero() {
                    cross_chain_assets.push(Asset {
                        amount: cross_chain_amount,
                        info: asset.info.clone(),
                    })
                }
            }

            // Send fee assets to fee collector.
            let (fee_funds, fee_cw20_transfer_messages) = forward_assets_direct(
                &fee_collection_assets,
                &cross_chain_outgoing_fee_config.fee_collector_addr,
            )?;
            response = response.add_messages(fee_cw20_transfer_messages);
            if !fee_funds.is_empty() {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: cross_chain_outgoing_fee_config
                        .fee_collector_addr
                        .to_string(),
                    amount: fee_funds,
                }))
            }

            // Initiate cross-chain transfer.
            for asset in cross_chain_assets {
                match &asset.info {
                    AssetInfo::NativeToken { denom } => {
                        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: wormhole_token_bridge_addr.to_string(),
                            msg: to_binary(&WormholeTokenBridgeExecuteMsg::DepositTokens {})?,
                            funds: vec![Coin {
                                amount: asset.amount,
                                denom: denom.clone(),
                            }],
                        }));
                    }
                    AssetInfo::Token { contract_addr } => {
                        response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: contract_addr.clone(),
                            msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
                                spender: wormhole_token_bridge_addr.to_string(),
                                amount: asset.amount,
                                expires: None,
                            })?,
                            funds: vec![],
                        }));
                    }
                }
                response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: wormhole_token_bridge_addr.to_string(),
                    msg: to_binary(&WormholeTokenBridgeExecuteMsg::InitiateTransfer {
                        asset,
                        recipient_chain,
                        recipient: recipient.clone(),
                        fee: Uint128::zero(),
                        nonce: 0u32,
                    })?,
                    funds: vec![],
                }));
            }
        }
    }
    Ok(response)
}

pub fn register_external_chain_manager(
    deps: DepsMut,
    info: MessageInfo,
    chain_id: ChainId,
    aperture_manager_addr: Vec<u8>,
) -> StdResult<Response> {
    if info.sender != ADMIN.load(deps.storage)? {
        return Err(StdError::generic_err("unauthorized"));
    }
    CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP.save(
        deps.storage,
        U16Key::from(chain_id),
        &aperture_manager_addr,
    )?;
    Ok(Response::default())
}

pub fn process_cross_chain_instruction(
    deps: DepsMut,
    env: Env,
    // VAA of an Aperture instruction message published by an external-chain Aperture manager.
    instruction_vaa: Binary,
    // VAAs of the accompanying token transfers.
    token_transfer_vaas: Vec<Binary>,
) -> StdResult<Response> {
    let parsed_instruction_vaa = get_parsed_vaa(deps.as_ref(), &env, &instruction_vaa)?;

    // Check that the instruction message is published by
    let expected_emitter_address = CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP.load(
        deps.storage,
        U16Key::from(parsed_instruction_vaa.emitter_chain),
    )?;
    if parsed_instruction_vaa.emitter_address != expected_emitter_address {
        return Err(StdError::generic_err(
            "unexpected instruction emitter address",
        ));
    }

    // Make sure that each instruction can only be successfully processed at most once.
    let completed = COMPLETED_INSTRUCTIONS
        .load(deps.storage, parsed_instruction_vaa.hash.as_slice())
        .unwrap_or(false);
    if completed {
        return Err(StdError::generic_err("instruction already completed"));
    }
    COMPLETED_INSTRUCTIONS.save(deps.storage, parsed_instruction_vaa.hash.as_slice(), &true)?;

    let instruction_payload_slice = parsed_instruction_vaa.payload.as_slice();
    let position = Position {
        chain_id: parsed_instruction_vaa.emitter_chain,
        position_id: Uint128::from(instruction_payload_slice.get_u128_be(0)),
    };

    let strategy_chain = instruction_payload_slice.get_u16(16);
    if strategy_chain != TERRA_CHAIN_ID {
        return Err(StdError::generic_err(
            "instruction not intended for Terra chain",
        ));
    }

    let mut assets = vec![];
    let num_token_transfers = instruction_payload_slice.get_u32(26) as usize;
    if num_token_transfers != token_transfer_vaas.len() {
        return Err(StdError::generic_err(
            "unexpected token_transfer_vaas length",
        ));
    }

    let mut response = Response::new();
    let wormhole_token_bridge_addr = WORMHOLE_TOKEN_BRIDGE_ADDR.load(deps.storage)?;
    for (i, token_transfer_vaa) in token_transfer_vaas.iter().enumerate() {
        let expected_sequence = instruction_payload_slice.get_u64((i << 3) + 30);
        assets.push(process_token_transfer_message(
            deps.as_ref(),
            &env,
            parsed_instruction_vaa.emitter_chain,
            expected_sequence,
            token_transfer_vaa,
        )?);

        // Attempt to complete the transfer; no-op if this transfer has already been completed.
        response = response.add_submessage(SubMsg {
            id: TOKEN_TRANSFER_SUBMIT_VAA_MSG_ID,
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: wormhole_token_bridge_addr.to_string(),
                funds: vec![],
                msg: to_binary(&WormholeTokenBridgeExecuteMsg::SubmitVaa {
                    data: token_transfer_vaas[i].clone(),
                })?,
            }),
            gas_limit: None,
            reply_on: cosmwasm_std::ReplyOn::Error,
        });
    }

    let encoded_action_len_index = (num_token_transfers << 3) + 30;
    let encoded_action_len = instruction_payload_slice.get_u32(encoded_action_len_index) as usize;
    let encoded_action_index = encoded_action_len_index + 4;
    let action_binary = Binary::from_base64(std::str::from_utf8(
        &instruction_payload_slice[encoded_action_index..encoded_action_index + encoded_action_len],
    )?)?;
    let action: Action = from_binary(&action_binary)?;

    if let Action::OpenPosition { data } = action {
        let strategy_id = StrategyId::from(instruction_payload_slice.get_u64(18));
        Ok(response.add_messages(save_new_position_info_and_open_it(
            deps,
            env,
            None,
            position,
            strategy_id,
            data,
            assets,
        )?))
    } else {
        Ok(response.add_messages(create_execute_strategy_messages(
            deps.as_ref(),
            env,
            None,
            position,
            action,
            assets,
        )?))
    }
}

fn process_token_transfer_message(
    deps: Deps,
    env: &Env,
    expected_emitter_chain: u16,
    expected_sequence: u64,
    token_transfer_vaa: &Binary,
) -> StdResult<Asset> {
    let parsed_token_transfer_vaa = get_parsed_vaa(deps, env, token_transfer_vaa)?;
    if expected_emitter_chain != parsed_token_transfer_vaa.emitter_chain {
        return Err(StdError::generic_err(
            "unexpected token transfer emitter chain",
        ));
    }

    // NOTE: no need to validate the emitter address; this is automatically done
    // when trying to complete the transfer
    if expected_sequence != parsed_token_transfer_vaa.sequence {
        return Err(StdError::generic_err("unexpected token transfer sequence"));
    }
    let token_bridge_message = TokenBridgeMessage::deserialize(&parsed_token_transfer_vaa.payload)?;
    if token_bridge_message.action != aperture_common::wormhole::Action::TRANSFER {
        return Err(StdError::generic_err("unexpected token transfer action"));
    }
    let transfer_info = TransferInfo::deserialize(&token_bridge_message.payload)?;
    if transfer_info.recipient_chain != TERRA_CHAIN_ID
        || transfer_info.recipient
            != extend_terra_address_to_32(
                &deps.api.addr_canonicalize(env.contract.address.as_str())?,
            )
    {
        return Err(StdError::generic_err("unexpected token transfer recipient"));
    }
    parse_token_transfer_asset(deps, transfer_info)
}

fn parse_token_transfer_asset(deps: Deps, transfer_info: TransferInfo) -> StdResult<Asset> {
    if transfer_info.token_chain != TERRA_CHAIN_ID {
        return Err(StdError::generic_err(
            "transferred token is not a Terra token",
        ));
    }
    let (_, mut amount) = transfer_info.amount;
    let (_, fee) = transfer_info.fee;
    amount = amount.checked_sub(fee).unwrap();

    static WORMHOLE_TERRA_NATIVE_TOKEN_INDICATOR: u8 = 1;
    let asset =
        if transfer_info.token_address.as_slice()[0] == WORMHOLE_TERRA_NATIVE_TOKEN_INDICATOR {
            let mut token_address = transfer_info.token_address;
            let token_address = token_address.as_mut_slice();
            token_address[0] = 0;
            let mut denom = token_address.to_vec();
            denom.retain(|&c| c != 0);
            Asset {
                info: AssetInfo::NativeToken {
                    denom: String::from_utf8(denom)?,
                },
                amount: Uint128::from(amount),
            }
        } else {
            Asset {
                info: AssetInfo::Token {
                    contract_addr: deps
                        .api
                        .addr_humanize(&transfer_info.token_address.as_slice().get_address(0))?
                        .to_string(),
                },
                amount: Uint128::from(amount),
            }
        };
    Ok(asset)
}

fn get_parsed_vaa(deps: Deps, env: &Env, vaa: &Binary) -> StdResult<ParsedVAA> {
    let wormhole_core_bridge_addr = WORMHOLE_CORE_BRIDGE_ADDR.load(deps.storage)?;
    deps.querier.query_wasm_smart(
        wormhole_core_bridge_addr.to_string(),
        &WormholeCoreBridgeQueryMsg::VerifyVAA {
            vaa: vaa.clone(),
            block_time: env.block.time.seconds(),
        },
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    if msg.id != TOKEN_TRANSFER_SUBMIT_VAA_MSG_ID {
        return Err(StdError::generic_err("unexpected reply id"));
    }

    if let ContractResult::Err(err) = msg.result {
        if err == "Generic error: VaaAlreadyExecuted: execute wasm contract failed" {
            // This means that this token transfer has already been successfully processed.
            Ok(Response::default())
        } else {
            Err(StdError::generic_err(err))
        }
    } else {
        Err(StdError::generic_err("unexpected success reply msg"))
    }
}
