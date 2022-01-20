use cosmwasm_std::{to_binary, Addr, Coin, CosmosMsg, StdResult, WasmMsg};
use terraswap::asset::{Asset, AssetInfo};

pub fn forward_assets_direct(
    assets: &[Asset],
    recipient: &Addr,
    cw20_increase_allowance_instead_of_transfer: bool,
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
                    msg: to_binary(&if cw20_increase_allowance_instead_of_transfer {
                        cw20::Cw20ExecuteMsg::IncreaseAllowance {
                            spender: recipient.to_string(),
                            amount: asset.amount,
                            expires: None,
                        }
                    } else {
                        cw20::Cw20ExecuteMsg::Transfer {
                            recipient: recipient.to_string(),
                            amount: asset.amount,
                        }
                    })?,
                }));
            }
        }
    }
    Ok((funds, msgs))
}
