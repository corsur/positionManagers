use std::convert::TryInto;

use cosmwasm_std::{from_binary, to_binary, Binary, StdError, StdResult, Uint128, Uint64};

use crate::common::{Action, SwapAndDisburseInfo};
use crate::constants::APERTURE_INSTRUCTION_VERSION;

pub struct StrategyInstructionInfo {
    pub position_id: Uint128,
    pub strategy_chain_id: u16,
    pub token_transfer_sequences: Vec<u64>,
}

pub enum ApertureInstruction {
    PositionOpenInstruction {
        strategy_info: StrategyInstructionInfo,
        strategy_id: Uint64,
        open_position_action_data: Option<Binary>,
    },
    ExecuteStrategyInstruction {
        strategy_info: StrategyInstructionInfo,
        action: Action,
    },
    SingleTokenDisbursementInstruction {
        // Sequence for the Wormhole token bridge transfer, e.g. transferring UST from Terra to Ethereum.
        sequence: u64,
        // Wormhole ID of the recipient chain.
        recipient_chain_id: u16,
        // Encoded recipient address (32-byte array).
        recipient_addr: [u8; WORMHOLE_ADDRESS_BYTES],
        // Information about the (optional) swap request.
        swap_info: Option<SwapAndDisburseInfo>,
    },
}

const INSTRUCTION_TYPE_POSITION_OPEN: u8 = 0;
const INSTRUCTION_TYPE_EXECUTE_STRATEGY: u8 = 1;
const INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT: u8 = 2;

const WORMHOLE_ADDRESS_BYTES: usize = 32;
const U128_NUM_BYTES: usize = 128 / 8;
const U64_NUM_BYTES: usize = 64 / 8;
const U32_NUM_BYTES: usize = 32 / 8;
const U16_NUM_BYTES: usize = 16 / 8;

/// Format of INSTRUCTION_TYPE_POSITION_OPEN:
/// | # of bytes | parsed field type | field name          | comment
/// |      1     |        u8         | version             | Value = 0.
/// |      1     |        u8         | instruction_type    | Value = INSTRUCTION_TYPE_POSITION_OPEN.
/// |     16     |       u128        | position_id         |
/// |      2     |        u16        | strategy_chain      |
/// |      4     |        u32        | num_token_transfers | Name this NTT for short.
/// |    8 * NTT |     u64 * NTT     | token_transfer_seq  | Sequence numbers of these NTT transfers.
/// |      4     |        u32        | encoded_data_len    | Length of the encoded position-opening action's `data` field. Name this EDL for short.
/// |    1 * EAL |      u8 * EDL     | encoded_data        | JSON-encoded string in base 64 for the `data` field. If EDL is zero, then `data` is None.
/// |      8     |        u64        | strategy_id         |

/// Format of INSTRUCTION_TYPE_EXECUTE_STRATEGY:
/// | # of bytes | parsed field type | field name          | comment
/// |      1     |        u8         | version             | Value = 0.
/// |      1     |        u8         | instruction_type    | Value = INSTRUCTION_TYPE_EXECUTE_STRATEGY.
/// |     16     |       u128        | position_id         |
/// |      2     |        u16        | strategy_chain      |
/// |      4     |        u32        | num_token_transfers | Name this NTT for short.
/// |    8 * NTT |     u64 * NTT     | token_transfer_seq  | Sequence numbers of these NTT transfers.
/// |      4     |        u32        | encoded_action_len  | Length of encoded action string. Name this EAL for short.
/// |    1 * EAL |      u8 * EAL     | encoded_action      | Action enum's JSON-encoded string in base 64.

/// Format of INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT:
/// | # of bytes | parsed field type | field name          | comment
/// |      1     |        u8         | version             | Value = 0.
/// |      1     |        u8         | instruction_type    | Value = INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT.
/// |      8     |        u64        | sequence            |
/// |      2     |        u16        | recipient_chain     |
/// |     32     |     [u8; 32]      | recipient           |
/// |     32     |     [u8; 32]      | swap.desired_token  | Only if `swap` is not None.
/// |     32     |       u256        | swap.minimum_amount | Only if `swap` is not None.
impl ApertureInstruction {
    pub fn deserialize(instruction_payload_slice: &[u8]) -> StdResult<Self> {
        let (version, rest_of_payload) = instruction_payload_slice.split_first().unwrap();
        if *version != APERTURE_INSTRUCTION_VERSION {
            return Err(StdError::generic_err("invalid or unsupported version"));
        }

        let (instruction_type, rest_of_payload) = rest_of_payload.split_first().unwrap();
        if *instruction_type != INSTRUCTION_TYPE_POSITION_OPEN
            && *instruction_type != INSTRUCTION_TYPE_EXECUTE_STRATEGY
        {
            return Err(StdError::generic_err(
                "unsupported instruction type for deserialization",
            ));
        }

        let (position_id_bytes, rest_of_payload) = rest_of_payload.split_at(U128_NUM_BYTES);
        let (strategy_chain_bytes, rest_of_payload) = rest_of_payload.split_at(U16_NUM_BYTES);

        let (num_token_transfers_bytes, mut rest_of_payload) =
            rest_of_payload.split_at(U32_NUM_BYTES);
        let num_token_transfers = u32::from_be_bytes(num_token_transfers_bytes.try_into().unwrap());
        let mut token_transfer_sequences = vec![];
        for _ in 0..num_token_transfers {
            // Note that the outer `rest_of_payload` has been marked as mutable so it can be modified inside this loop.
            // If we simply do `let (_, rest_of_payload) = ...` here, a new `rest_of_payload` is created that shadows the one in the outer loop, and will not persist across loop iterations.
            // A new reference named `new_rest_of_payload` is introduced here to temporarily store the new reference before `rest_of_payload` is modified.
            let (sequence_bytes, new_rest_of_payload) = rest_of_payload.split_at(U64_NUM_BYTES);
            rest_of_payload = new_rest_of_payload;
            token_transfer_sequences.push(u64::from_be_bytes(sequence_bytes.try_into().unwrap()));
        }

        let strategy_instruction_info = StrategyInstructionInfo {
            position_id: Uint128::from(u128::from_be_bytes(position_id_bytes.try_into().unwrap())),
            strategy_chain_id: u16::from_be_bytes(strategy_chain_bytes.try_into().unwrap()),
            token_transfer_sequences,
        };

        match *instruction_type {
            INSTRUCTION_TYPE_POSITION_OPEN => {
                let (data_len_bytes, rest_of_payload) = rest_of_payload.split_at(U32_NUM_BYTES);
                let data_len = u32::from_be_bytes(data_len_bytes.try_into().unwrap()) as usize;
                let (data_bytes, rest_of_payload) = rest_of_payload.split_at(data_len);
                let open_position_action_data = if data_len > 0 {
                    Some(Binary::from_base64(std::str::from_utf8(data_bytes)?)?)
                } else {
                    None
                };

                if rest_of_payload.len() != U64_NUM_BYTES {
                    return Err(StdError::generic_err("invalid instruction payload length"));
                }
                Ok(ApertureInstruction::PositionOpenInstruction {
                    strategy_id: Uint64::from(u64::from_be_bytes(
                        rest_of_payload.try_into().unwrap(),
                    )),
                    strategy_info: strategy_instruction_info,
                    open_position_action_data,
                })
            }
            INSTRUCTION_TYPE_EXECUTE_STRATEGY => {
                let (action_len_bytes, rest_of_payload) = rest_of_payload.split_at(U32_NUM_BYTES);
                let action_len = u32::from_be_bytes(action_len_bytes.try_into().unwrap()) as usize;
                let (action_bytes, rest_of_payload) = rest_of_payload.split_at(action_len);
                let action: Action =
                    from_binary(&Binary::from_base64(std::str::from_utf8(action_bytes)?)?)?;
                if !rest_of_payload.is_empty() {
                    return Err(StdError::generic_err("invalid instruction payload length"));
                }
                if let Action::OpenPosition { .. } = action {
                    return Err(StdError::generic_err(
                        "open-position action on an existing position is disallowed",
                    ));
                }
                Ok(ApertureInstruction::ExecuteStrategyInstruction {
                    strategy_info: strategy_instruction_info,
                    action,
                })
            }
            _ => unreachable!(),
        }
    }

    pub fn serialize(&self) -> StdResult<Vec<u8>> {
        match self {
            ApertureInstruction::PositionOpenInstruction {
                strategy_info,
                strategy_id,
                open_position_action_data,
            } => {
                let mut bytes = [
                    [APERTURE_INSTRUCTION_VERSION, INSTRUCTION_TYPE_POSITION_OPEN].as_slice(),
                    strategy_info.position_id.u128().to_be_bytes().as_slice(),
                    strategy_info.strategy_chain_id.to_be_bytes().as_slice(),
                    (strategy_info.token_transfer_sequences.len() as u32)
                        .to_be_bytes()
                        .as_slice(),
                ]
                .concat();
                for sequence in &strategy_info.token_transfer_sequences {
                    bytes.extend(sequence.to_be_bytes());
                }
                if let Some(data) = open_position_action_data {
                    let encoded_data = data.to_base64();
                    bytes.extend((encoded_data.len() as u32).to_be_bytes());
                    bytes.extend(encoded_data.as_bytes());
                } else {
                    bytes.extend(0u32.to_be_bytes());
                }
                bytes.extend(strategy_id.u64().to_be_bytes());
                Ok(bytes)
            }
            ApertureInstruction::ExecuteStrategyInstruction {
                strategy_info,
                action,
            } => {
                let mut bytes = [
                    [
                        APERTURE_INSTRUCTION_VERSION,
                        INSTRUCTION_TYPE_EXECUTE_STRATEGY,
                    ]
                    .as_slice(),
                    strategy_info.position_id.u128().to_be_bytes().as_slice(),
                    strategy_info.strategy_chain_id.to_be_bytes().as_slice(),
                    (strategy_info.token_transfer_sequences.len() as u32)
                        .to_be_bytes()
                        .as_slice(),
                ]
                .concat();
                for sequence in &strategy_info.token_transfer_sequences {
                    bytes.extend(sequence.to_be_bytes());
                }
                let encoded_action = to_binary(action)?.to_base64();
                bytes.extend((encoded_action.len() as u32).to_be_bytes());
                bytes.extend(encoded_action.as_bytes());
                Ok(bytes)
            }
            ApertureInstruction::SingleTokenDisbursementInstruction {
                sequence,
                recipient_chain_id,
                recipient_addr,
                swap_info,
            } => {
                let mut bytes = [
                    [
                        APERTURE_INSTRUCTION_VERSION,
                        INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT,
                    ]
                    .as_slice(),
                    sequence.to_be_bytes().as_slice(),
                    recipient_chain_id.to_be_bytes().as_slice(),
                    recipient_addr.as_slice(),
                ]
                .concat();
                if let Some(swap_info) = swap_info {
                    if swap_info.desired_token_addr.len() != WORMHOLE_ADDRESS_BYTES {
                        return Err(StdError::generic_err(
                            "invalid desired token address length",
                        ));
                    }
                    bytes.extend(swap_info.desired_token_addr.as_slice());
                    bytes.extend(swap_info.minimum_amount.to_be_bytes());
                }
                Ok(bytes)
            }
        }
    }
}
