use cosmwasm_std::{to_binary, Addr, BankMsg, Coin, CosmosMsg, StdResult, Uint128, WasmMsg};
use terraswap::asset::{Asset, AssetInfo};

use crate::{common::Recipient, wormhole::WormholeTokenBridgeExecuteMsg};

pub fn initiate_outgoing_token_transfers(
    wormhole_token_bridge_addr: &Addr,
    assets: Vec<Asset>,
    recipient: Recipient,
) -> StdResult<Vec<CosmosMsg>> {
    let mut msgs = vec![];
    match recipient {
        Recipient::TerraChain { recipient } => {
            for asset in assets {
                match &asset.info {
                    AssetInfo::NativeToken { denom } => {
                        msgs.push(CosmosMsg::Bank(BankMsg::Send {
                            to_address: recipient.clone(),
                            amount: vec![Coin {
                                amount: asset.amount,
                                denom: denom.clone(),
                            }],
                        }));
                    }
                    AssetInfo::Token { contract_addr } => {
                        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: contract_addr.clone(),
                            msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                                amount: asset.amount,
                                recipient: recipient.clone(),
                            })?,
                            funds: vec![],
                        }));
                    }
                }
            }
        }
        Recipient::ExternalChain {
            recipient_chain,
            recipient,
        } => {
            for asset in assets {
                match &asset.info {
                    AssetInfo::NativeToken { denom } => {
                        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: wormhole_token_bridge_addr.to_string(),
                            msg: to_binary(&WormholeTokenBridgeExecuteMsg::DepositTokens {})?,
                            funds: vec![Coin {
                                amount: asset.amount,
                                denom: denom.clone(),
                            }],
                        }));
                    }
                    AssetInfo::Token { contract_addr } => {
                        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: contract_addr.clone(),
                            msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
                                spender: wormhole_token_bridge_addr.to_string(),
                                amount: asset.amount,
                                expires: None,
                            })?,
                            funds: vec![],
                        }));
                    }
                }
                msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: wormhole_token_bridge_addr.to_string(),
                    msg: to_binary(&WormholeTokenBridgeExecuteMsg::InitiateTransfer {
                        asset,
                        recipient_chain,
                        recipient: recipient.clone(),
                        fee: Uint128::zero(),
                        nonce: 0u32,
                    })?,
                    funds: vec![],
                }));
            }
        }
    }
    Ok(msgs)
}
