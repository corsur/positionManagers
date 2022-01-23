use aperture_common::wormhole::{ParsedVAA, WormholeCoreBridgeQueryMsg};
use cosmwasm_std::testing::MockStorage;
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Api, Binary, CanonicalAddr, ContractResult, Empty,
    OwnedDeps, Querier, QuerierResult, QueryRequest, RecoverPubkeyError, StdResult, SystemError,
    SystemResult, VerificationError, WasmQuery,
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
    fn addr_validate(&self, _human: &str) -> StdResult<Addr> {
        unimplemented!()
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
                            payload: vec![
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0,
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 184, 101, 121, 74, 106, 98, 71,
                                57, 122, 90, 86, 57, 119, 98, 51, 78, 112, 100, 71, 108, 118, 98,
                                105, 73, 54, 101, 121, 74, 121, 90, 87, 78, 112, 99, 71, 108, 108,
                                98, 110, 81, 105, 79, 110, 115, 105, 90, 88, 104, 48, 90, 88, 74,
                                117, 89, 87, 120, 102, 89, 50, 104, 104, 97, 87, 52, 105, 79, 110,
                                115, 105, 99, 109, 86, 106, 97, 88, 66, 112, 90, 87, 53, 48, 88,
                                50, 78, 111, 89, 87, 108, 117, 73, 106, 111, 120, 77, 68, 65, 119,
                                77, 83, 119, 105, 99, 109, 86, 106, 97, 88, 66, 112, 90, 87, 53,
                                48, 73, 106, 111, 105, 81, 85, 70, 66, 81, 85, 70, 66, 81, 85, 70,
                                66, 81, 85, 70, 66, 81, 85, 70, 66, 81, 87, 70, 75, 98, 71, 104,
                                90, 83, 84, 66, 48, 89, 48, 86, 109, 77, 86, 70, 83, 75, 50, 82,
                                114, 81, 86, 82, 86, 85, 85, 86, 84, 89, 51, 90, 84, 89, 122, 48,
                                105, 102, 88, 49, 57, 102, 81, 61, 61,
                            ],
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
                            payload: vec![
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                1, 0, 0, 0, 0, 0, 0, 1, 22, 0, 0, 1, 92, 101, 119, 111, 74, 73, 109, 57, 119, 90, 87,
                                53, 102, 99, 71, 57, 122, 97, 88, 82, 112, 98, 50, 52, 105, 79, 105, 66, 55, 67, 103,
                                107, 74, 73, 109, 82, 104, 100, 71, 69, 105, 79, 105, 65, 105, 90, 88, 100, 118, 90,
                                48, 108, 68, 81, 87, 100, 74, 98, 108, 74, 111, 89, 50, 49, 107, 98, 71, 82, 71, 79,
                                88, 82, 104, 86, 122, 86, 109, 87, 84, 73, 53, 99, 50, 74, 72, 82, 106, 66, 97, 87, 69,
                                112, 111, 89, 107, 89, 53, 101, 86, 108, 89, 85, 110, 66, 105, 101, 85, 107, 50, 83,
                                85, 78, 74, 101, 85, 120, 113, 84, 87, 108, 77, 81, 87, 57, 110, 83, 85, 78, 66, 90,
                                48, 108, 117, 85, 109, 104, 106, 98, 87, 82, 115, 90, 69, 89, 53, 100, 70, 108, 89, 97,
                                71, 90, 90, 77, 106, 108, 122, 89, 107, 100, 71, 77, 70, 112, 89, 83, 109, 104, 105,
                                82, 106, 108, 53, 87, 86, 104, 83, 99, 71, 74, 53, 83, 84, 90, 74, 81, 48, 108, 53, 84,
                                71, 112, 106, 97, 85, 120, 66, 98, 50, 100, 74, 81, 48, 70, 110, 83, 87, 48, 120, 99,
                                71, 78, 117, 83, 110, 90, 106, 98, 68, 108, 111, 89, 122, 78, 79, 98, 71, 82, 71, 79,
                                87, 112, 107, 101, 107, 108, 51, 87, 68, 74, 71, 97, 49, 112, 73, 83, 87, 108, 80, 97,
                                85, 70, 112, 90, 69, 100, 87, 101, 87, 78, 116, 82, 88, 104, 108, 87, 69, 48, 119, 87,
                                107, 104, 107, 77, 50, 86, 116, 82, 109, 120, 105, 98, 88, 66, 117, 84, 87, 49, 107,
                                78, 85, 49, 69, 83, 110, 82, 106, 77, 110, 104, 48, 87, 88, 112, 114, 77, 108, 112,
                                113, 83, 84, 74, 79, 77, 50, 103, 121, 89, 48, 104, 79, 99, 86, 108, 89, 85, 84, 78,
                                97, 77, 50, 100, 112, 81, 50, 52, 119, 80, 83, 73, 75, 67, 88, 48, 75, 102, 81, 61, 61,
                            ],
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
            _ => panic!(),
        }
    }

    pub fn new(wormhole_core_bridge: &str) -> Self {
        WasmMockQuerier {
            wormhole_core_bridge: wormhole_core_bridge.to_string(),
        }
    }
}
