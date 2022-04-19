use aperture_common::{
    byte_util::{extend_terra_address_to_32, ByteUtils},
    common::{ChainId, Position, Recipient},
    constants::WORMHOLE_NONCE,
    instruction::ApertureInstruction,
    terra_manager::TERRA_CHAIN_ID,
    token_util::{forward_assets_direct, validate_and_accept_incoming_asset_transfer},
    wormhole::{
        ParsedVAA, TokenBridgeMessage, TransferInfo, WormholeCoreBridgeExecuteMsg,
        WormholeCoreBridgeQueryMsg, WormholeTokenBridgeExecuteMsg,
    },
};
use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Coin, ContractResult, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw_storage_plus::U16Key;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    state::{
        ADMIN, CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP, COMPLETED_INSTRUCTIONS,
        CROSS_CHAIN_OUTGOING_FEE_CONFIG, WORMHOLE_CORE_BRIDGE_ADDR, WORMHOLE_TOKEN_BRIDGE_ADDR,
    },
    terra_chain::{create_execute_strategy_messages, save_new_position_info_and_open_it},
    util::get_next_sequence,
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
            recipient_chain_id,
            recipient_addr,
            swap_info,
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

            // Publish serialized token disbursement instruction.
            let instruction_bytes = ApertureInstruction::SingleTokenDisbursementInstruction {
                sequence: get_next_sequence(deps, &wormhole_token_bridge_addr)?,
                recipient_chain_id,
                recipient_addr: recipient_addr.to_array()?,
                swap_info,
            }
            .serialize()?;
            response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: WORMHOLE_CORE_BRIDGE_ADDR.load(deps.storage)?.to_string(),
                funds: vec![],
                msg: to_binary(&WormholeCoreBridgeExecuteMsg::PostMessage {
                    message: Binary::from(instruction_bytes),
                    nonce: WORMHOLE_NONCE,
                })?,
            }));

            // Transfer token to Aperture manager contract on the recipient chain.
            if cross_chain_assets.len() != 1 {
                return Err(StdError::generic_err(
                    "only single-token cross-chain disbursement is supported at this time",
                ));
            }
            let asset = &cross_chain_assets[0];
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
                    asset: asset.clone(),
                    recipient_chain: recipient_chain_id,
                    recipient: Binary::from(
                        CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP
                            .load(deps.storage, U16Key::from(recipient_chain_id))?,
                    ),
                    fee: Uint128::zero(),
                    nonce: WORMHOLE_NONCE,
                })?,
                funds: vec![],
            }));
        }
    }
    Ok(response)
}

#[test]
fn test_initiate_outgoing_token_transfer() {
    use crate::mock_querier::custom_mock_dependencies;
    use crate::state::CrossChainOutgoingFeeConfig;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{Addr, BankMsg, Decimal};

    let uusd_coin = Coin {
        denom: String::from("uusd"),
        amount: Uint128::from(1001u128),
    };
    let mut deps = custom_mock_dependencies("wormhole_core_bridge");
    let assets = vec![Asset {
        amount: Uint128::from(1001u128),
        info: AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    }];

    // Terra chain recipient.
    let response = initiate_outgoing_token_transfer(
        deps.as_ref(),
        mock_env(),
        mock_info("sender", &[uusd_coin.clone()]),
        assets.clone(),
        Recipient::TerraChain {
            recipient: String::from("terra1recipient"),
        },
    )
    .unwrap();
    assert_eq!(response.messages.len(), 1);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Bank(BankMsg::Send {
            amount: vec![uusd_coin.clone()],
            to_address: String::from("terra1recipient")
        })
    );

    // External chain recipient.
    // With a fee rate of 0.1%, the 1001 uusd transfer results in a fee in the amount of 1 uusd; the remaining 1000 uusd transfer is initiated.
    CROSS_CHAIN_OUTGOING_FEE_CONFIG
        .save(
            deps.as_mut().storage,
            &CrossChainOutgoingFeeConfig {
                rate: Decimal::from_ratio(1u128, 1000u128),
                fee_collector_addr: Addr::unchecked("terra1collector"),
            },
        )
        .unwrap();
    WORMHOLE_CORE_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_core_bridge"),
        )
        .unwrap();
    WORMHOLE_TOKEN_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_token_bridge"),
        )
        .unwrap();

    let recipient_chain_id = 5u16;
    let recipient_addr = [1u8; 32];
    let aperture_manager = [2u8; 32];
    CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP
        .save(
            deps.as_mut().storage,
            U16Key::from(recipient_chain_id),
            &aperture_manager,
        )
        .unwrap();

    let response = initiate_outgoing_token_transfer(
        deps.as_ref(),
        mock_env(),
        mock_info("sender", &[uusd_coin.clone()]),
        assets.clone(),
        Recipient::ExternalChain {
            recipient_chain_id,
            recipient_addr: Binary::from(recipient_addr),
            swap_info: None,
        },
    )
    .unwrap();

    /*
    let instruction_bytes = ApertureInstruction::SingleTokenDisbursementInstruction {
        sequence: 10u64,
        recipient_chain_id,
        recipient_addr,
        swap_info: None,
    }
    .serialize()
    .unwrap();*/
    let instruction_bytes = vec![
        0, 2, 0, 0, 0, 0, 0, 0, 0, 10, 0, 5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];

    assert_eq!(response.messages.len(), 4);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Bank(BankMsg::Send {
            amount: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(1u128)
            }],
            to_address: String::from("terra1collector")
        })
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_core_bridge"),
            msg: to_binary(&WormholeCoreBridgeExecuteMsg::PostMessage {
                message: Binary::from(instruction_bytes.clone()),
                nonce: WORMHOLE_NONCE,
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        response.messages[2].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_token_bridge"),
            msg: to_binary(&WormholeTokenBridgeExecuteMsg::DepositTokens {}).unwrap(),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(1000u128)
            }],
        })
    );
    assert_eq!(
        response.messages[3].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_token_bridge"),
            msg: to_binary(&WormholeTokenBridgeExecuteMsg::InitiateTransfer {
                asset: Asset {
                    amount: Uint128::from(1000u128),
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd")
                    }
                },
                recipient_chain: recipient_chain_id,
                recipient: Binary::from(aperture_manager.to_vec()),
                fee: Uint128::zero(),
                nonce: WORMHOLE_NONCE,
            })
            .unwrap(),
            funds: vec![],
        })
    );

    // External chain recipient, cw20 transfer, and with a small transfer amount that results in zero fees.
    let assets = vec![Asset {
        amount: Uint128::from(999u128),
        info: AssetInfo::Token {
            contract_addr: String::from("terra1cw20"),
        },
    }];
    let response = initiate_outgoing_token_transfer(
        deps.as_ref(),
        mock_env(),
        mock_info("sender", &[]),
        assets.clone(),
        Recipient::ExternalChain {
            recipient_chain_id,
            recipient_addr: Binary::from(recipient_addr),
            swap_info: None,
        },
    )
    .unwrap();
    assert_eq!(response.messages.len(), 4);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("terra1cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::TransferFrom {
                owner: String::from("sender"),
                recipient: mock_env().contract.address.to_string(),
                amount: Uint128::from(999u128)
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_core_bridge"),
            msg: to_binary(&WormholeCoreBridgeExecuteMsg::PostMessage {
                message: Binary::from(instruction_bytes),
                nonce: WORMHOLE_NONCE,
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        response.messages[2].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("terra1cw20"),
            msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
                spender: String::from("wormhole_token_bridge"),
                amount: Uint128::from(999u128),
                expires: None,
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        response.messages[3].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_token_bridge"),
            msg: to_binary(&WormholeTokenBridgeExecuteMsg::InitiateTransfer {
                asset: assets[0].clone(),
                recipient_chain: recipient_chain_id,
                recipient: Binary::from(aperture_manager),
                fee: Uint128::zero(),
                nonce: WORMHOLE_NONCE,
            })
            .unwrap(),
            funds: vec![],
        })
    );
}

pub fn register_external_chain_manager(
    deps: DepsMut,
    info: MessageInfo,
    chain_id: ChainId,
    aperture_manager_addr: Binary,
) -> StdResult<Response> {
    if info.sender != ADMIN.load(deps.storage)? {
        return Err(StdError::generic_err("unauthorized"));
    }
    CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP.save(
        deps.storage,
        U16Key::from(chain_id),
        &aperture_manager_addr.to_array()?,
    )?;
    Ok(Response::default())
}

#[test]
fn test_register_external_chain_manager() {
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::Addr;

    let mut deps = mock_dependencies(&[]);
    ADMIN
        .save(deps.as_mut().storage, &Addr::unchecked("admin"))
        .unwrap();

    // Unauthorized call.
    assert_eq!(
        register_external_chain_manager(
            deps.as_mut(),
            mock_info("sender", &[]),
            1,
            Binary::from(vec![3, 2, 1])
        )
        .unwrap_err(),
        StdError::generic_err("unauthorized")
    );

    // Authorized call but incorrect length.
    assert_eq!(
        register_external_chain_manager(
            deps.as_mut(),
            mock_info("admin", &[]),
            1,
            Binary::from(vec![3, 2, 1])
        )
        .unwrap_err(),
        StdError::invalid_data_size(32, 3)
    );

    // Authorized call and correct length.
    assert_eq!(
        register_external_chain_manager(
            deps.as_mut(),
            mock_info("admin", &[]),
            1,
            Binary::from(vec![1; 32])
        )
        .unwrap(),
        Response::default()
    );
    assert_eq!(
        CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP
            .load(deps.as_ref().storage, U16Key::from(1))
            .unwrap(),
        [1; 32]
    );
}

/// Processes an instruction published by Aperture manager on another chain.
///
/// The instruction is a generic message published by the external-chain Aperture manager via Wormhole.
/// If the instruction carries a position action that requires token transfers, e.g. position open or increase,
/// the associated token transfers, via Wormhole token bridge, are provided by `token_transfer_vaas`.
/// The instruction message payload encodes sufficient information for us to verify that these token transfers
/// are intended to be consumed to fulfill this particular instruction.
///
/// Validation criteria for the instruction message:
/// (1) Emitter address is the registered Aperture manager address for the emitter chain.
///     Registration is performed by the administrator via register_external_chain_manager().
/// (2) This particular instruction has not been successfully processed before. This prevents replay of the same instruction multiple times.
/// (3) The `strategy_chain` field is populated with TERRA_CHAIN_ID, showing that this instruction is intended for the Terra Aperture manager.
/// (4) Attached token transfer sequence numbers match the token transfer VAAs, in the same order.
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

    let instruction = ApertureInstruction::deserialize(&parsed_instruction_vaa.payload)?;
    let strategy_instruction_info = match &instruction {
        ApertureInstruction::PositionOpenInstruction { strategy_info, .. } => strategy_info,
        ApertureInstruction::ExecuteStrategyInstruction { strategy_info, .. } => strategy_info,
        _ => unreachable!("unsupported instruction type should not have been deserialized"),
    };

    if strategy_instruction_info.strategy_chain_id != TERRA_CHAIN_ID {
        return Err(StdError::generic_err(
            "instruction not intended for Terra chain",
        ));
    }

    if strategy_instruction_info.token_transfer_sequences.len() != token_transfer_vaas.len() {
        return Err(StdError::generic_err(
            "unexpected token_transfer_vaas length",
        ));
    }

    let mut response = Response::new();
    let mut assets = vec![];
    let wormhole_token_bridge_addr = WORMHOLE_TOKEN_BRIDGE_ADDR.load(deps.storage)?;
    for (token_transfer_vaa, expected_sequence) in token_transfer_vaas
        .iter()
        .zip(strategy_instruction_info.token_transfer_sequences.iter())
    {
        assets.push(process_token_transfer_message(
            deps.as_ref(),
            &env,
            parsed_instruction_vaa.emitter_chain,
            *expected_sequence,
            token_transfer_vaa,
        )?);

        // Attempt to complete the transfer; no-op if this transfer has already been completed successfully.
        // If someone has already independently submitted this token transfer's VAA and completed this transfer,
        // then the token amount has already been added to this contract's balance, so we should treat this situation
        // as a success in reply() and proceed to fulfilling the instruction.
        // We should revert this transaction when WormholeTokenBridgeExecuteMsg::SubmitVaa returns any other error.
        response = response.add_submessage(SubMsg {
            id: TOKEN_TRANSFER_SUBMIT_VAA_MSG_ID,
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: wormhole_token_bridge_addr.to_string(),
                funds: vec![],
                msg: to_binary(&WormholeTokenBridgeExecuteMsg::SubmitVaa {
                    data: token_transfer_vaa.clone(),
                })?,
            }),
            gas_limit: None,
            reply_on: cosmwasm_std::ReplyOn::Error,
        });
    }

    let position = Position {
        chain_id: parsed_instruction_vaa.emitter_chain,
        position_id: strategy_instruction_info.position_id,
    };
    match instruction {
        ApertureInstruction::PositionOpenInstruction {
            strategy_id,
            open_position_action_data,
            ..
        } => Ok(response.add_messages(save_new_position_info_and_open_it(
            deps,
            env,
            None,
            position,
            strategy_id,
            open_position_action_data,
            assets,
        )?)),
        ApertureInstruction::ExecuteStrategyInstruction { action, .. } => Ok(response
            .add_messages(create_execute_strategy_messages(
                deps.as_ref(),
                env,
                None,
                position,
                action,
                assets,
            )?)),
        _ => {
            unreachable!("unsupported instruction type; code execution shouldn't have reached here")
        }
    }
}

/// Validation criteria for a token transfer message:
/// (1) Emitter address is the registered Wormhole token bridge address on the emitter chain.
///     This validation is performed by Wormhole token bridge Terra contract when completing the transfer, so there is no need for us to validate this again.
/// (2) Emitter chain is the same as the emitter chain of the instruction.
/// (3) The sequence number of this token transfer VAA matches what's encoded in the instruction.
/// (4) The recipient address is that of Aperture Terra manager (this contract).
/// (5) The transfered token is a Terra token.
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

    // See https://github.com/certusone/wormhole/blob/c2a879ec7cbafffe9e2d4c037a78123f7d0f7df2/terra/contracts/token-bridge/src/contract.rs#L632
    // for information on how Wormhole token bridge encodes Terra native and cw20 tokens.
    static WORMHOLE_TERRA_NATIVE_TOKEN_INDICATOR: u8 = 1;
    let asset =
        if transfer_info.token_address.as_slice()[0] == WORMHOLE_TERRA_NATIVE_TOKEN_INDICATOR {
            // See https://github.com/certusone/wormhole/blob/c2a879ec7cbafffe9e2d4c037a78123f7d0f7df2/terra/contracts/token-bridge/src/contract.rs#L810
            // for information on how Wormhole token bridge decodes a Terra native token's denomination.
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
            // See https://github.com/certusone/wormhole/blob/c2a879ec7cbafffe9e2d4c037a78123f7d0f7df2/terra/contracts/token-bridge/src/contract.rs#L724
            // for information on how Wormhole token bridge decodes a Terra cw20 token address.
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

pub fn get_parsed_vaa(deps: Deps, env: &Env, vaa: &Binary) -> StdResult<ParsedVAA> {
    let wormhole_core_bridge_addr = WORMHOLE_CORE_BRIDGE_ADDR.load(deps.storage)?;
    deps.querier.query_wasm_smart(
        wormhole_core_bridge_addr.to_string(),
        &WormholeCoreBridgeQueryMsg::VerifyVAA {
            vaa: vaa.clone(),
            block_time: env.block.time.seconds(),
        },
    )
}

#[test]
fn test_process_cross_chain_instruction_close_position() {
    use crate::mock_querier::custom_mock_dependencies;
    use crate::state::{POSITION_TO_STRATEGY_LOCATION_MAP, STRATEGY_ID_TO_METADATA_MAP};
    use aperture_common::common::{
        get_position_key, Action, StrategyLocation, StrategyPositionManagerExecuteMsg,
    };
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Uint64};
    use cw_storage_plus::U64Key;

    let mut deps = custom_mock_dependencies("wormhole_core_bridge");
    WORMHOLE_CORE_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_core_bridge"),
        )
        .unwrap();
    WORMHOLE_TOKEN_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_token_bridge"),
        )
        .unwrap();
    CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP
        .save(
            deps.as_mut().storage,
            U16Key::from(10001),
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 233, 112, 219, 235, 53, 127, 85, 58, 144,
                106, 20, 222, 5, 18, 37, 187, 26, 238, 73,
            ],
        )
        .unwrap();
    POSITION_TO_STRATEGY_LOCATION_MAP
        .save(
            deps.as_mut().storage,
            get_position_key(&Position {
                chain_id: 10001,
                position_id: Uint128::zero(),
            }),
            &StrategyLocation::TerraChain(Uint64::zero()),
        )
        .unwrap();
    STRATEGY_ID_TO_METADATA_MAP
        .save(
            deps.as_mut().storage,
            U64Key::from(0),
            &aperture_common::common::StrategyMetadata {
                name: String::from("DN"),
                version: String::from("v0"),
                manager_addr: Addr::unchecked("strategy_manager"),
            },
        )
        .unwrap();

    let response = process_cross_chain_instruction(
        deps.as_mut(),
        mock_env(),
        Binary::from_base64("AQAAAAABAFLbAJeL535FIPx9E5lq8H6aNUubBKJr2zRm0QlOmx4hT3fwD0mYf5IUTnjtw4oV+/1iIgkUahYzyYULRbV60KUAYeysZwAUNfQnEQAAAAAAAAAAAAAAAGrpcNvrNX9VOpBqFN4FEiW7Gu5JAAAAAAAAAAEBAAAAAAAAAAAAAAAAAAAAAAADAAAAAAAAAAAAAAAAAAAAuGV5SmpiRzl6WlY5d2IzTnBkR2x2YmlJNmV5SnlaV05wY0dsbGJuUWlPbnNpWlhoMFpYSnVZV3hmWTJoaGFXNGlPbnNpY21WamFYQnBaVzUwWDJOb1lXbHVJam94TURBd01Td2ljbVZqYVhCcFpXNTBJam9pUVVGQlFVRkJRVUZCUVVGQlFVRkJRV0ZLYkdoWlNUQjBZMFZtTVZGU0syUnJRVlJWVVVWVFkzWlRZejBpZlgxOWZRPT0=").unwrap(),
        vec![],
    )
    .unwrap();
    assert_eq!(response.messages.len(), 1);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("strategy_manager"),
            msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
                position: Position {
                    chain_id: 10001,
                    position_id: Uint128::zero(),
                },
                action: Action::ClosePosition {
                    recipient: Recipient::ExternalChain {
                        recipient_chain_id: 10001,
                        recipient_addr: Binary::from([3u8; 32]),
                        swap_info: None,
                    }
                },
                assets: vec![],
            })
            .unwrap(),
            funds: vec![],
        })
    );
}

#[test]
fn test_process_cross_chain_instruction_open_position() {
    use crate::mock_querier::custom_mock_dependencies;
    use crate::state::{POSITION_TO_STRATEGY_LOCATION_MAP, STRATEGY_ID_TO_METADATA_MAP};
    use aperture_common::common::{
        get_position_key, Action, StrategyLocation, StrategyPositionManagerExecuteMsg,
    };
    use aperture_common::delta_neutral_position_manager::DeltaNeutralParams;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Addr, Decimal, Uint64};
    use cw_storage_plus::U64Key;

    let mut deps = custom_mock_dependencies("wormhole_core_bridge");
    WORMHOLE_CORE_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_core_bridge"),
        )
        .unwrap();
    WORMHOLE_TOKEN_BRIDGE_ADDR
        .save(
            deps.as_mut().storage,
            &Addr::unchecked("wormhole_token_bridge"),
        )
        .unwrap();
    CHAIN_ID_TO_APERTURE_MANAGER_ADDRESS_MAP
        .save(
            deps.as_mut().storage,
            U16Key::from(10001),
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 130, 190, 119, 130, 59, 86, 190, 176, 84, 62,
                154, 118, 154, 32, 103, 134, 108, 225, 13, 14,
            ],
        )
        .unwrap();
    POSITION_TO_STRATEGY_LOCATION_MAP
        .save(
            deps.as_mut().storage,
            get_position_key(&Position {
                chain_id: 10001,
                position_id: Uint128::zero(),
            }),
            &StrategyLocation::TerraChain(Uint64::zero()),
        )
        .unwrap();
    STRATEGY_ID_TO_METADATA_MAP
        .save(
            deps.as_mut().storage,
            U64Key::from(0),
            &aperture_common::common::StrategyMetadata {
                name: String::from("DN"),
                version: String::from("v0"),
                manager_addr: Addr::unchecked("strategy_manager"),
            },
        )
        .unwrap();

    let token_transfer_vaa = Binary::from_base64("AQAAAAABADhqQkDb0KlwGvLA9fpBZrOKaa4ty35jXC7lG6zz9dNteb73ItRp5UMS5smzOEX4Xi6VwNhU4/dqHNQGrwW6xCMBYeyVsQDzszEnEQAAAAAAAAAAAAAAAPF0+ag3U2xEkyHfHKCTu5aUjVOGAAAAAAAAARYPAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAjw0YAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHV1c2QAAwAAAAAAAAAAAAAAAOAGQQe87Y6/y/IuPqR8pYBQYEmNAAMAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==").unwrap();
    let response = process_cross_chain_instruction(
        deps.as_mut(),
        mock_env(),
        Binary::from_base64("AQAAAAABAOWWxynoIu8CJjRjj0bHcPFCytTQ4n9XjmciENEboHToc1vvZkvNK706tUbbGDD3cgE9+qdaiktDkhipuquaLPAAYeyVsQAUNfQnEQAAAAAAAAAAAAAAAIK+d4I7Vr6wVD6adpogZ4Zs4Q0OAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAADAAAAAAAAAAAAAAABAAAAAAAAARYAAAFcZXdvSkltOXdaVzVmY0c5emFYUnBiMjRpT2lCN0Nna0pJbVJoZEdFaU9pQWlaWGR2WjBsRFFXZEpibEpvWTIxa2JHUkdPWFJoVnpWbVdUSTVjMkpIUmpCYVdFcG9Za1k1ZVZsWVVuQmllVWsyU1VOSmVVeHFUV2xNUVc5blNVTkJaMGx1VW1oamJXUnNaRVk1ZEZsWWFHWlpNamx6WWtkR01GcFlTbWhpUmpsNVdWaFNjR0o1U1RaSlEwbDVUR3BqYVV4QmIyZEpRMEZuU1cweGNHTnVTblpqYkRsb1l6Tk9iR1JHT1dwa2VrbDNXREpHYTFwSVNXbFBhVUZwWkVkV2VXTnRSWGhsV0Uwd1draGtNMlZ0Um14aWJYQnVUVzFrTlUxRVNuUmpNbmgwV1hwck1scHFTVEpPTTJneVkwaE9jVmxZVVROYU0yZHBRMjR3UFNJS0NYMEtmUT09").unwrap(),
        vec![token_transfer_vaa.clone()],
    )
    .unwrap();
    assert_eq!(response.messages.len(), 2);
    assert_eq!(
        response.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("wormhole_token_bridge"),
            msg: to_binary(&WormholeTokenBridgeExecuteMsg::SubmitVaa {
                data: token_transfer_vaa,
            })
            .unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        response.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("strategy_manager"),
            msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
                position: Position {
                    chain_id: 10001,
                    position_id: Uint128::zero(),
                },
                action: Action::OpenPosition {
                    data: Some(
                        to_binary(&DeltaNeutralParams {
                            target_min_collateral_ratio: Decimal::from_ratio(23u128, 10u128),
                            target_max_collateral_ratio: Decimal::from_ratio(27u128, 10u128),
                            mirror_asset_cw20_addr: String::from(
                                "terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"
                            ),
                            allow_off_market_position_open: None,
                        })
                        .unwrap()
                    ),
                },
                assets: vec![Asset {
                    info: AssetInfo::NativeToken {
                        denom: String::from("uusd"),
                    },
                    amount: Uint128::from(600000000u128)
                }],
            })
            .unwrap(),
            funds: vec![Coin {
                denom: String::from("uusd"),
                amount: Uint128::from(600000000u128),
            }],
        })
    );
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
        // Since we chose to only reply on error, this should never happen.
        Err(StdError::generic_err("unexpected success reply msg"))
    }
}
