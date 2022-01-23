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
                            timestamp: 0,
                            nonce: 0,
                            len_signers: 2,
                            emitter_chain: 10001,
                            emitter_address: vec![],
                            sequence: 100,
                            consistency_level: 0,
                            payload: vec![],
                            hash: vec![0],
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
