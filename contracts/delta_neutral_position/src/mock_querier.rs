use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, BalanceResponse, BankQuery, Coin, ContractResult,
    Decimal, Empty, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128,
    WasmQuery,
};
use terraswap::asset::AssetInfo;

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
    pub mirror_lock: String,
    pub mirror_mint: String,
    pub mirror_oracle: String,
    pub mirror_staking: String,
    pub mirror_collateral_oracle: String,
    pub anchor_market: String,
    pub spectrum_mirror_farms: String,
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
                } else if *address == String::from("this") && denom == "uusd" {
                    SystemResult::Ok(ContractResult::Ok(
                        to_binary(&BalanceResponse {
                            amount: Coin {
                                denom: String::from("uusd"),
                                amount: Uint128::from(10u128),
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
                        mirror_protocol::mint::QueryMsg::Position { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(mirror_protocol::mint::PositionResponse {
                                        idx: Uint128::from(1u128),
                                        owner: String::from("owner"),
                                        collateral: terraswap::asset::Asset {
                                            info: AssetInfo::Token {
                                                contract_addr: String::from("aust_cw20"),
                                            },
                                            amount: Uint128::from(9000u128),
                                        },
                                        asset: terraswap::asset::Asset {
                                            info: AssetInfo::Token {
                                                contract_addr: self.cw20_token.to_string(),
                                            },
                                            amount: Uint128::from(5000u128),
                                        },
                                        is_short: true,
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
                        _ => panic!(),
                    }
                } else if contract_addr == &self.mirror_lock {
                    let msg: mirror_protocol::lock::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        mirror_protocol::lock::QueryMsg::PositionLockInfo { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(mirror_protocol::lock::PositionLockInfoResponse {
                                        idx: Uint128::from(1u128),
                                        receiver: String::from("this"),
                                        locked_amount: Uint128::from(20u128),
                                        unlock_time: 12345,
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
                } else if contract_addr == &self.mirror_collateral_oracle {
                    let msg: mirror_protocol::collateral_oracle::QueryMsg =
                        from_binary(&msg).unwrap();
                    match msg {
                        mirror_protocol::collateral_oracle::QueryMsg::CollateralPrice {
                            ..
                        } => SystemResult::Ok(ContractResult::Ok(
                            to_binary(
                                &(mirror_protocol::collateral_oracle::CollateralPriceResponse {
                                    rate: Decimal::from_ratio(11u128, 10u128),
                                    asset: String::from("aust_cw20"),
                                    last_updated: 0,
                                    multiplier: Decimal::one(),
                                    is_revoked: false,
                                }),
                            )
                            .unwrap(),
                        )),
                        _ => panic!(),
                    }
                } else if contract_addr == &self.mirror_staking {
                    let msg: mirror_protocol::staking::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        mirror_protocol::staking::QueryMsg::RewardInfo { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(mirror_protocol::staking::RewardInfoResponse {
                                        staker_addr: String::from("this"),
                                        reward_infos: vec![
                                            mirror_protocol::staking::RewardInfoResponseItem {
                                                asset_token: self.cw20_token.to_string(),
                                                bond_amount: Uint128::from(1u128),
                                                pending_reward: Uint128::from(3u128),
                                                is_short: true,
                                            },
                                        ],
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
                } else if contract_addr == &self.spectrum_mirror_farms {
                    let msg: spectrum_protocol::mirror_farm::QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        spectrum_protocol::mirror_farm::QueryMsg::reward_info { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(spectrum_protocol::mirror_farm::RewardInfoResponse {
                                        staker_addr: String::from("staker"),
                                        reward_infos: vec![spectrum_protocol::mirror_farm::RewardInfoResponseItem {
                                            asset_token: self.cw20_token.to_string(),
                                            farm_share_index: Decimal::zero(),
                                            auto_spec_share_index: Decimal::zero(),
                                            stake_spec_share_index: Decimal::zero(),
                                            bond_amount: Uint128::from(1u128),
                                            auto_bond_amount: Uint128::from(1u128),
                                            stake_bond_amount: Uint128::zero(),
                                            farm_share: Uint128::from(1u128),
                                            spec_share: Uint128::from(1u128),
                                            auto_bond_share: Uint128::from(1u128),
                                            stake_bond_share: Uint128::from(1u128),
                                            pending_farm_reward: Uint128::zero(),
                                            pending_spec_reward: Uint128::from(5u128)
                                        }],
                                    }),
                                )
                                .unwrap(),
                            ))
                        }
                        _ => panic!(),
                    }
                } else if contract_addr == &String::from("lp_token") {
                    let msg: cw20::Cw20QueryMsg = from_binary(&msg).unwrap();
                    match msg {
                        cw20::Cw20QueryMsg::TokenInfo { .. } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_binary(
                                    &(cw20::TokenInfoResponse {
                                        name: String::from("lp token"),
                                        symbol: String::from("lp"),
                                        decimals: 6,
                                        total_supply: Uint128::from(1000u128),
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
            mirror_lock: String::from("mirror_lock"),
            mirror_mint: String::from("mirror_mint"),
            mirror_oracle: String::from("mirror_oracle"),
            mirror_staking: String::from("mirror_staking"),
            mirror_collateral_oracle: String::from("mirror_collateral_oracle"),
            anchor_market: String::from("anchor_market"),
            spectrum_mirror_farms: String::from("spectrum_mirror_farms"),
        }
    }
}
