use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    from_slice,
    testing::{MockApi, MockStorage},
    to_binary, BalanceResponse, BankQuery, Coin, ContractResult, Empty, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};

pub fn custom_mock_dependencies(
    anchor_market_addr: &str,
    anchor_ust_cw20_addr: &str,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(anchor_market_addr, anchor_ust_cw20_addr);

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    anchor_market_addr: String,
    anchor_ust_cw20_addr: String,
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
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                if address == &self.anchor_market_addr && denom == "uusd" {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(&BalanceResponse {
                            amount: Coin {
                                denom: String::from("uusd"),
                                amount: Uint128::from(1050u128),
                            },
                        })
                        .unwrap(),
                    ))
                } else if address == "this" && denom == "uusd" {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(&BalanceResponse {
                            amount: Coin {
                                denom: String::from("uusd"),
                                amount: Uint128::from(29u128),
                            },
                        })
                        .unwrap(),
                    ))
                } else {
                    panic!()
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr,
                msg: _,
            }) => {
                if contract_addr == &self.anchor_market_addr {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(moneymarket::market::StateResponse {
                                total_liabilities: Decimal256::from_ratio(
                                    Uint256::from(100u128),
                                    Uint256::one(),
                                ),
                                total_reserves: Decimal256::from_ratio(
                                    Uint256::from(50u128),
                                    Uint256::one(),
                                ),
                                last_interest_updated: 0u64,
                                last_reward_updated: 0u64,
                                global_interest_index: Decimal256::zero(),
                                global_reward_index: Decimal256::zero(),
                                anc_emission_rate: Decimal256::zero(),
                                prev_aterra_supply: Uint256::zero(),
                                prev_exchange_rate: Decimal256::zero(),
                            }),
                        )
                        .unwrap(),
                    ))
                } else if contract_addr == &self.anchor_ust_cw20_addr {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(cw20::TokenInfoResponse {
                                name: String::new(),
                                symbol: String::new(),
                                decimals: 6,
                                total_supply: Uint128::from(1000u128),
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

    pub fn new(anchor_market_addr: &str, anchor_ust_cw20_addr: &str) -> Self {
        WasmMockQuerier {
            anchor_market_addr: anchor_market_addr.to_string(),
            anchor_ust_cw20_addr: anchor_ust_cw20_addr.to_string(),
        }
    }
}
