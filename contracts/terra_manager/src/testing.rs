use crate::contract::{instantiate, reply};
use crate::msg::InstantiateMsg;
use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::NFT_ADDR;

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{to_binary, Addr, Reply, ReplyOn, SubMsg, SubMsgExecutionResponse, WasmMsg};
use protobuf::Message;

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

#[test]
fn test_reply() {
    // Baisc test setup.
    let mut deps = mock_dependencies(&[]);
    let mut instantiate_response = MsgInstantiateContractResponse::new();
    instantiate_response.set_contract_address(MOCK_CONTRACT_ADDR.to_string());

    let reply_msg = Reply {
        id: 1,
        result: cosmwasm_std::ContractResult::Ok(SubMsgExecutionResponse {
            events: vec![],
            data: Some(
                // Convert into binary parseable by Protobuf.
                Message::write_to_bytes(&instantiate_response)
                    .unwrap()
                    .into(),
            ),
        }),
    };

    // Trigger reply's side effect. Response is not needed.
    let _reply_response = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // Upon successfully reply() execution, we can check mutated state.
    assert_eq!(
        NFT_ADDR.load(deps.as_mut().storage).unwrap(),
        MOCK_CONTRACT_ADDR
    );
}
