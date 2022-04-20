use cosmwasm_std::{to_binary, Addr, Coin, CosmosMsg, Env, MessageInfo, StdResult, WasmMsg};
use terraswap::asset::{Asset, AssetInfo};

pub fn forward_assets_direct(
    assets: &[Asset],
    recipient: &Addr,
) -> StdResult<(Vec<Coin>, Vec<CosmosMsg>)> {
    let mut msgs = vec![];
    let mut funds = vec![];
    for asset in assets {
        match &asset.info {
            AssetInfo::NativeToken { denom } => {
                funds.push(Coin {
                    amount: asset.amount,
                    denom: denom.clone(),
                });
            }
            AssetInfo::Token { contract_addr } => {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.clone(),
                    funds: vec![],
                    msg: to_binary(&cw20::Cw20ExecuteMsg::Transfer {
                        recipient: recipient.to_string(),
                        amount: asset.amount,
                    })?,
                }));
            }
        }
    }
    Ok((funds, msgs))
}

pub fn validate_and_accept_incoming_asset_transfer(
    env: Env,
    info: MessageInfo,
    assets: &[Asset],
) -> StdResult<Vec<CosmosMsg>> {
    let mut msgs = vec![];
    for asset in assets {
        match &asset.info {
            AssetInfo::NativeToken { .. } => {
                asset.assert_sent_native_token_balance(&info)?;
            }
            AssetInfo::Token { contract_addr } => {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&cw20::Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: asset.amount,
                    })?,
                    funds: vec![],
                }));
            }
        }
    }
    Ok(msgs)
}
