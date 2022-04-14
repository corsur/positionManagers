use aperture_common::common::Action;
use aperture_common::delta_neutral_position_manager::DeltaNeutralParams;
use aperture_common::instruction::{ApertureInstruction, StrategyInstructionInfo};
use aperture_common::terra_manager::TERRA_CHAIN_ID;
use aperture_common::wormhole::{ParsedVAA, WormholeCoreBridgeQueryMsg};
use cosmwasm_std::testing::MockStorage;
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Api, Binary, CanonicalAddr, ContractResult, Decimal,
    Empty, OwnedDeps, Querier, QuerierResult, QueryRequest, RecoverPubkeyError, StdResult,
    SystemError, SystemResult, Uint128, Uint64, VerificationError, WasmQuery,
};

pub fn custom_mock_dependencies(
    wormhole_core_bridge: &str,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(wormhole_core_bridge);

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

impl Default for MockApi {
    fn default() -> Self {
        MockApi {}
    }
}

pub struct MockApi {}

impl Api for MockApi {
    fn addr_validate(&self, human: &str) -> StdResult<Addr> {
        Ok(Addr::unchecked(human))
    }

    fn addr_canonicalize(&self, _human: &str) -> StdResult<CanonicalAddr> {
        Ok(CanonicalAddr(Binary(vec![
            224, 6, 65, 7, 188, 237, 142, 191, 203, 242, 46, 62, 164, 124, 165, 128, 80, 96, 73,
            141,
        ])))
    }

    fn addr_humanize(&self, _canonical: &CanonicalAddr) -> StdResult<Addr> {
        unimplemented!()
    }

    fn secp256k1_verify(
        &self,
        _message_hash: &[u8],
        _signature: &[u8],
        _public_key: &[u8],
    ) -> Result<bool, VerificationError> {
        unimplemented!()
    }

    fn secp256k1_recover_pubkey(
        &self,
        _message_hash: &[u8],
        _signature: &[u8],
        _recovery_param: u8,
    ) -> Result<Vec<u8>, RecoverPubkeyError> {
        unimplemented!()
    }

    fn ed25519_verify(
        &self,
        _message: &[u8],
        _signature: &[u8],
        _public_key: &[u8],
    ) -> Result<bool, VerificationError> {
        unimplemented!()
    }

    fn ed25519_batch_verify(
        &self,
        _messages: &[&[u8]],
        _signatures: &[&[u8]],
        _public_keys: &[&[u8]],
    ) -> Result<bool, VerificationError> {
        unimplemented!()
    }

    fn debug(&self, message: &str) {
        println!("{}", message);
    }
}

pub struct WasmMockQuerier {
    wormhole_core_bridge: String,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr == &self.wormhole_core_bridge {
                    let wormhole_query_msg: WormholeCoreBridgeQueryMsg = from_binary(msg).unwrap();
                    let WormholeCoreBridgeQueryMsg::VerifyVAA { vaa, block_time: _ } =
                        wormhole_query_msg;
                    let parsed_vaa = if vaa == Binary::from_base64("AQAAAAABAFLbAJeL535FIPx9E5lq8H6aNUubBKJr2zRm0QlOmx4hT3fwD0mYf5IUTnjtw4oV+/1iIgkUahYzyYULRbV60KUAYeysZwAUNfQnEQAAAAAAAAAAAAAAAGrpcNvrNX9VOpBqFN4FEiW7Gu5JAAAAAAAAAAEBAAAAAAAAAAAAAAAAAAAAAAADAAAAAAAAAAAAAAAAAAAAuGV5SmpiRzl6WlY5d2IzTnBkR2x2YmlJNmV5SnlaV05wY0dsbGJuUWlPbnNpWlhoMFpYSnVZV3hmWTJoaGFXNGlPbnNpY21WamFYQnBaVzUwWDJOb1lXbHVJam94TURBd01Td2ljbVZqYVhCcFpXNTBJam9pUVVGQlFVRkJRVUZCUVVGQlFVRkJRV0ZLYkdoWlNUQjBZMFZtTVZGU0syUnJRVlJWVVVWVFkzWlRZejBpZlgxOWZRPT0=").unwrap() {
                        ParsedVAA {
                            version: 1,
                            guardian_set_index: 0,
                            timestamp: 1642900583,
                            nonce: 1324532,
                            len_signers: 1,
                            emitter_chain: 10001,
                            emitter_address: vec![
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 233, 112, 219, 235, 53,
                                127, 85, 58, 144, 106, 20, 222, 5, 18, 37, 187, 26, 238, 73,
                            ],
                            sequence: 1,
                            consistency_level: 1,
                            payload: ApertureInstruction::ExecuteStrategyInstruction {
                                strategy_info: StrategyInstructionInfo {
                                    position_id: Uint128::zero(),
                                    strategy_chain_id: TERRA_CHAIN_ID,
                                    token_transfer_sequences: vec![],
                                },
                                action: Action::ClosePosition {
                                    recipient: aperture_common::common::Recipient::ExternalChain {
                                        recipient_chain_id: 10001u16,
                                        recipient_addr: Binary::from([3u8; 32]),
                                        swap_info: None
                                    }
                                }
                            }.serialize().unwrap(),
                            hash: vec![
                                42, 238, 135, 196, 222, 30, 20, 186, 72, 16, 245, 12, 214, 47, 37,
                                245, 236, 60, 233, 11, 15, 225, 64, 177, 37, 142, 162, 31, 192,
                                163, 236, 151,
                            ],
                        }
                    } else if vaa == Binary::from_base64("AQAAAAABAOWWxynoIu8CJjRjj0bHcPFCytTQ4n9XjmciENEboHToc1vvZkvNK706tUbbGDD3cgE9+qdaiktDkhipuquaLPAAYeyVsQAUNfQnEQAAAAAAAAAAAAAAAIK+d4I7Vr6wVD6adpogZ4Zs4Q0OAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAADAAAAAAAAAAAAAAABAAAAAAAAARYAAAFcZXdvSkltOXdaVzVmY0c5emFYUnBiMjRpT2lCN0Nna0pJbVJoZEdFaU9pQWlaWGR2WjBsRFFXZEpibEpvWTIxa2JHUkdPWFJoVnpWbVdUSTVjMkpIUmpCYVdFcG9Za1k1ZVZsWVVuQmllVWsyU1VOSmVVeHFUV2xNUVc5blNVTkJaMGx1VW1oamJXUnNaRVk1ZEZsWWFHWlpNamx6WWtkR01GcFlTbWhpUmpsNVdWaFNjR0o1U1RaSlEwbDVUR3BqYVV4QmIyZEpRMEZuU1cweGNHTnVTblpqYkRsb1l6Tk9iR1JHT1dwa2VrbDNXREpHYTFwSVNXbFBhVUZwWkVkV2VXTnRSWGhsV0Uwd1draGtNMlZ0Um14aWJYQnVUVzFrTlUxRVNuUmpNbmgwV1hwck1scHFTVEpPTTJneVkwaE9jVmxZVVROYU0yZHBRMjR3UFNJS0NYMEtmUT09").unwrap() {
                        ParsedVAA {
                            version: 1,
                            guardian_set_index: 0,
                            timestamp: 1642894769,
                            nonce: 1324532,
                            len_signers: 1,
                            emitter_chain: 10001,
                            emitter_address: vec![
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 130, 190, 119, 130, 59, 86, 190, 176, 84, 62, 154,
                                118, 154, 32, 103, 134, 108, 225, 13, 14,
                            ],
                            sequence: 0,
                            consistency_level: 1,
                            payload: ApertureInstruction::PositionOpenInstruction {
                                strategy_info: StrategyInstructionInfo {
                                    position_id: Uint128::zero(),
                                    strategy_chain_id: TERRA_CHAIN_ID,
                                    token_transfer_sequences: vec![278u64],
                                },
                                strategy_id: Uint64::zero(),
                                open_position_action_data: Some(to_binary(&DeltaNeutralParams {
                                    target_min_collateral_ratio: Decimal::from_ratio(23u128, 10u128),
                                    target_max_collateral_ratio: Decimal::from_ratio(27u128, 10u128),
                                    mirror_asset_cw20_addr: String::from("terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"),
                                }).unwrap()),
                            }.serialize().unwrap(),
                            hash: vec![
                                84, 182, 22, 10, 172, 171, 230, 223, 139, 109, 213, 137, 217, 43, 56, 7, 63, 111, 93,
                                70, 193, 137, 68, 30, 179, 116, 2, 244, 197, 228, 214, 137,
                            ],
                        }
                    } else if vaa == Binary::from_base64("AQAAAAABADhqQkDb0KlwGvLA9fpBZrOKaa4ty35jXC7lG6zz9dNteb73ItRp5UMS5smzOEX4Xi6VwNhU4/dqHNQGrwW6xCMBYeyVsQDzszEnEQAAAAAAAAAAAAAAAPF0+ag3U2xEkyHfHKCTu5aUjVOGAAAAAAAAARYPAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAjw0YAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHV1c2QAAwAAAAAAAAAAAAAAAOAGQQe87Y6/y/IuPqR8pYBQYEmNAAMAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==").unwrap() {
                        ParsedVAA {
                            version: 1,
                            guardian_set_index: 0,
                            timestamp: 1642894769,
                            nonce: 15971121,
                            len_signers: 1,
                            emitter_chain: 10001,
                            emitter_address: vec![
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 241, 116, 249, 168, 55, 83, 108, 68, 147, 33, 223,
                                28, 160, 147, 187, 150, 148, 141, 83, 134,
                            ],
                            sequence: 278,
                            consistency_level: 15,
                            payload: vec![
                                1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                35, 195, 70, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0, 0, 0, 117, 117, 115, 100, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 224, 6, 65,
                                7, 188, 237, 142, 191, 203, 242, 46, 62, 164, 124, 165, 128, 80, 96, 73, 141, 0, 3, 0,
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0,
                            ],
                            hash: vec![
                                225, 182, 187, 20, 151, 73, 41, 242, 7, 102, 237, 194, 124, 5, 98, 16, 112, 9, 34, 162,
                                0, 20, 162, 91, 46, 75, 49, 109, 16, 88, 23, 190,
                            ],
                        }
                    } else {
                        panic!()
                    };
                    SystemResult::Ok(ContractResult::Ok(to_binary(&parsed_vaa).unwrap()))
                } else {
                    panic!()
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr,
                key: _,
            }) => {
                // Returning a mock sequence number of 10u64.
                if contract_addr == &self.wormhole_core_bridge {
                    SystemResult::Ok(ContractResult::Ok(to_binary(&10u64).unwrap()))
                } else {
                    panic!();
                }
            }
            _ => {
                panic!("unknown request: {:?}", request)
            }
        }
    }

    pub fn new(wormhole_core_bridge: &str) -> Self {
        WasmMockQuerier {
            wormhole_core_bridge: wormhole_core_bridge.to_string(),
        }
    }
}
