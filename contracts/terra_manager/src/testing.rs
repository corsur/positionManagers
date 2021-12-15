use crate::contract::instantiate;
use crate::msg::InstantiateMsg;

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{to_binary, Addr, ReplyOn, SubMsg, WasmMsg};

#[test]
fn test_initialization() {
    let code_id: u64 = 1234;
    let mut deps = mock_dependencies(&[]);
    let mut env = mock_env();
    // Explicit set env's contract address.
    env.contract.address = Addr::unchecked(MOCK_CONTRACT_ADDR);
    let msg = InstantiateMsg { code_id: code_id };
    let init_response = instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        msg,
    )
    .unwrap();
    assert_eq!(
        init_response.messages,
        vec![SubMsg {
            msg: WasmMsg::Instantiate {
                admin: None,
                code_id: code_id,
                msg: to_binary(&cw721_base::InstantiateMsg {
                    name: "Aperture NFT".to_string(),
                    symbol: "APT_NFT".to_string(),
                    // Minter will be the Terra Manager itself.
                    minter: MOCK_CONTRACT_ADDR.to_string(),
                })
                .unwrap(),
                funds: vec![],
                label: String::new(),
            }
            .into(),
            gas_limit: None,
            id: 1, // The reply id.
            reply_on: ReplyOn::Success,
        }]
    );
}
