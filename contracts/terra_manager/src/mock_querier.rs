use cosmwasm_std::testing::{MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, ContractResult, Empty, OwnedDeps, Querier, QuerierResult, QueryRequest,
    SystemError, SystemResult,
};

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn custom_mock_dependencies() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new();

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {}

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
            QueryRequest::Wasm(_) => SystemResult::Ok(ContractResult::Ok(
                to_binary(&cw721::OwnerOfResponse {
                    owner: MOCK_CONTRACT_ADDR.to_string(),
                    approvals: vec![],
                })
                .unwrap(),
            )),
            _ => panic!("DO NOT ENTER HERE"),
        }
    }
}

impl WasmMockQuerier {
    pub fn new() -> Self {
        WasmMockQuerier {}
    }
}
