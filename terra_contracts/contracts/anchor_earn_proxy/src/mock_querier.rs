use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    from_slice,
    testing::{MockApi, MockStorage},
    to_binary, ContractResult, Empty, OwnedDeps, Querier, QuerierResult, QueryRequest, SystemError,
    SystemResult, WasmQuery,
};

pub fn custom_mock_dependencies(
    anchor_market_addr: &str,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(anchor_market_addr);

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    anchor_market_addr: String,
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
        match request {
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr,
                msg: _,
            }) => {
                if contract_addr == &self.anchor_market_addr {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(moneymarket::market::EpochStateResponse {
                                exchange_rate: Decimal256::from_ratio(11, 10),
                                aterra_supply: Uint256::from(1000u128),
                            }),
                        )
                        .unwrap(),
                    ))
                } else {
                    panic!()
                }
            }
            _ => panic!(),
        }
    }

    pub fn new(anchor_market_addr: &str) -> Self {
        WasmMockQuerier {
            anchor_market_addr: anchor_market_addr.to_string(),
        }
    }
}
