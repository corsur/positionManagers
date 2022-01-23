use aperture_common::wormhole::{ParsedVAA, WormholeCoreBridgeQueryMsg};
use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, ContractResult, Empty, OwnedDeps, Querier, QuerierResult,
    QueryRequest, SystemError, SystemResult, WasmQuery,
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
                    let _wormhole_query_msg: WormholeCoreBridgeQueryMsg = from_binary(msg).unwrap();
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(&ParsedVAA {
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
                        })
                        .unwrap(),
                    ))
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
