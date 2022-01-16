use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, BalanceResponse, BankQuery, Coin, ContractResult,
    Empty, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};

pub struct WasmMockQuerier {
    pub terraswap_factory: String,
    pub astroport_factory: String,
    pub terraswap_pair: String,
    pub astroport_pair: String,
    pub terraswap_return_amount: Uint128,
    pub astroport_return_amount: Uint128,
    pub cw20_token: String,
    pub terraswap_pool_cw20_balance: Uint128,
    pub terraswap_pool_uusd_balance: Uint128,
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
                if *address == self.terraswap_pair && denom == "uusd" {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(&BalanceResponse {
                            amount: Coin {
                                denom: String::from("uusd"),
                                amount: self.terraswap_pool_uusd_balance,
                            },
                        })
                        .unwrap(),
                    ))
                } else {
                    panic!()
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr == &self.terraswap_factory {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(terraswap::asset::PairInfo {
                                asset_infos: [
                                    terraswap::asset::AssetInfo::Token {
                                        contract_addr: self.cw20_token.clone(),
                                    },
                                    terraswap::asset::AssetInfo::NativeToken {
                                        denom: String::from("uusd"),
                                    },
                                ],
                                contract_addr: self.terraswap_pair.clone(),
                                liquidity_token: String::from("lp_token"),
                            }),
                        )
                        .unwrap(),
                    ))
                } else if contract_addr == &self.astroport_factory {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(astroport::asset::PairInfo {
                                pair_type: astroport::factory::PairType::Xyk {},
                                asset_infos: [
                                    astroport::asset::AssetInfo::Token {
                                        contract_addr: Addr::unchecked(self.cw20_token.clone()),
                                    },
                                    astroport::asset::AssetInfo::NativeToken {
                                        denom: String::from("uusd"),
                                    },
                                ],
                                contract_addr: Addr::unchecked(self.astroport_pair.clone()),
                                liquidity_token: Addr::unchecked("unused"),
                            }),
                        )
                        .unwrap(),
                    ))
                } else if contract_addr == &self.terraswap_pair {
                    let msg: terraswap::pair::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        terraswap::pair::QueryMsg::Simulation { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(terraswap::pair::SimulationResponse {
                                        return_amount: self.terraswap_return_amount,
                                        spread_amount: Uint128::zero(),
                                        commission_amount: Uint128::zero(),
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
                        _ => {
                            panic!()
                        }
                    }
                } else if contract_addr == &self.astroport_pair {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(
                            &(astroport::pair::SimulationResponse {
                                return_amount: self.astroport_return_amount,
                                spread_amount: Uint128::zero(),
                                commission_amount: Uint128::zero(),
                            }),
                        )
                        .unwrap(),
                    ))
                } else if contract_addr == &self.cw20_token {
                    let msg: cw20::Cw20QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        cw20::Cw20QueryMsg::Balance { .. } => SystemResult::Ok(ContractResult::Ok(
                            to_binary(
                                &(cw20::BalanceResponse {
                                    balance: self.terraswap_pool_cw20_balance,
                                }),
                            )
                            .unwrap(),
                        )),
                        _ => panic!(),
                    }
                } else {
                    panic!()
                }
            }
            _ => panic!(),
        }
    }

    pub fn new(
        terraswap_factory: String,
        astroport_factory: String,
        terraswap_pair: String,
        astroport_pair: String,
        terraswap_return_amount: Uint128,
        astroport_return_amount: Uint128,
        cw20_token: String,
        terraswap_pool_cw20_balance: Uint128,
        terraswap_pool_uusd_balance: Uint128,
    ) -> Self {
        WasmMockQuerier {
            terraswap_factory,
            astroport_factory,
            terraswap_pair,
            astroport_pair,
            terraswap_return_amount,
            astroport_return_amount,
            cw20_token,
            terraswap_pool_cw20_balance,
            terraswap_pool_uusd_balance,
        }
    }
}
