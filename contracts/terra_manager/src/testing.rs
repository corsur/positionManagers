use crate::contract::{execute, instantiate, query, reply};
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::msg_instantiate_contract_response::MsgInstantiateContractResponse;
use crate::state::{NEXT_STRATEGY_ID, NFT_ADDR};

use aperture_common::common::StrategyMetadata;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Addr, DepsMut, Reply, ReplyOn, SubMsg, SubMsgExecutionResponse, Uint64,
    WasmMsg,
};
use protobuf::Message;

fn init(deps: DepsMut) {
    let msg = InstantiateMsg { code_id: 1234u64 };
    let _res = instantiate(deps, mock_env(), mock_info(MOCK_CONTRACT_ADDR, &[]), msg).unwrap();
}

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
    let _res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();

    // Upon successfully reply() execution, we can check mutated state.
    assert_eq!(
        NFT_ADDR.load(deps.as_mut().storage).unwrap(),
        MOCK_CONTRACT_ADDR
    );
}

#[test]
fn test_add_strategy() {
    let mut deps = mock_dependencies(&[]);
    init(deps.as_mut());

    let msg = ExecuteMsg::AddStrategy {
        name: "test_strat".to_string(),
        version: "1.0.1".to_string(),
        manager_addr: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g".to_string(),
    };

    // Position id should be 0 intially.
    assert_eq!(
        NEXT_STRATEGY_ID.load(deps.as_mut().storage).unwrap(),
        Uint64::from(0u64)
    );

    let _res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        msg,
    );
    // Position id should be 1 now.
    assert_eq!(
        NEXT_STRATEGY_ID.load(deps.as_mut().storage).unwrap(),
        Uint64::from(1u64)
    );

    // Test querying metadata against newly added strategy.
    let query_response = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::GetStrategyMetadata {
            strategy_id: Uint64::from(0u64),
        },
    )
    .unwrap();

    let parsed_query_response: StrategyMetadata = from_binary(&query_response).unwrap();
    assert_eq!(
        parsed_query_response,
        StrategyMetadata {
            name: "test_strat".to_string(),
            version: "1.0.1".to_string(),
            manager_addr: Addr::unchecked("terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g"),
        }
    );

    // Now, we remove the strategy.
    let _remove_res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        ExecuteMsg::RemoveStrategy {
            strategy_id: Uint64::from(0u64),
        },
    )
    .unwrap();

    // Next position id should remain unchanged.
    assert_eq!(
        NEXT_STRATEGY_ID.load(deps.as_mut().storage).unwrap(),
        Uint64::from(1u64)
    );

    let bad_query_response = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::GetStrategyMetadata {
            strategy_id: Uint64::from(0u64),
        },
    );
    assert!(
        bad_query_response.is_err(),
        "Strategy metadata should not exist."
    );
}
