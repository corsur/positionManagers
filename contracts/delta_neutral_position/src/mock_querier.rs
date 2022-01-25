use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, BalanceResponse, BankQuery, Coin, ContractResult,
    Decimal, Empty, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128,
    WasmQuery,
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
    pub mirror_mint: String,
    pub mirror_oracle: String,
    pub anchor_market: String,
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
                } else if contract_addr == &self.mirror_mint {
                    let msg: mirror_protocol::mint::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        mirror_protocol::mint::QueryMsg::AssetConfig { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(mirror_protocol::mint::AssetConfigResponse {
                                        token: String::from("token"),
                                        auction_discount: Decimal::zero(),
                                        min_collateral_ratio: Decimal::from_ratio(15u128, 10u128),
                                        end_price: None,
                                        ipo_params: None,
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
                        _ => panic!(),
                    }
                } else if contract_addr == &self.mirror_oracle {
                    let msg: mirror_protocol::oracle::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        mirror_protocol::oracle::QueryMsg::Price { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(mirror_protocol::oracle::PriceResponse {
                                        rate: Decimal::from_ratio(10u128, 1u128),
                                        last_updated_base: 0,
                                        last_updated_quote: 0,
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
                        _ => panic!(),
                    }
                } else if contract_addr == &self.anchor_market {
                    let msg: moneymarket::market::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        moneymarket::market::QueryMsg::EpochState { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(moneymarket::market::EpochStateResponse {
                                        exchange_rate: Decimal256::from_ratio(11, 10),
                                        aterra_supply: Uint256::from(1000u128),
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
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
            mirror_mint: String::from("mirror_mint"),
            mirror_oracle: String::from("mirror_oracle"),
            anchor_market: String::from("anchor_market"),
        }
    }
}
