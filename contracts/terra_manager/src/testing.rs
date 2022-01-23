use crate::contract::{execute, instantiate, query};
use crate::mock_querier::custom_mock_dependencies;
use crate::state::NEXT_STRATEGY_ID;
use aperture_common::terra_manager::{ExecuteMsg, InstantiateMsg, QueryMsg, TERRA_CHAIN_ID};

use aperture_common::common::{
    Action, Position, Recipient, Strategy, StrategyMetadata, StrategyPositionManagerExecuteMsg,
};
use aperture_common::delta_neutral_position_manager::DeltaNeutralParams;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, Decimal, ReplyOn, SubMsg, Uint128, Uint64, WasmMsg,
};

#[test]
fn test_initialization() {
    let mut deps = mock_dependencies(&[]);
    let mut env = mock_env();
    // Explicit set env's contract address.
    env.contract.address = Addr::unchecked(MOCK_CONTRACT_ADDR);
    let msg = InstantiateMsg {
        admin_addr: String::from("admin"),
        wormhole_core_bridge_addr: String::from("mock_wormhole_core_bridge"),
        wormhole_token_bridge_addr: String::from("mock_wormhole_token_bridge"),
        cross_chain_outgoing_fee_rate: Decimal::from_ratio(1u128, 1000u128),
        cross_chain_outgoing_fee_collector_addr: String::from("mock_fee_collector"),
    };
    let init_response = instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        msg,
    )
    .unwrap();
    assert_eq!(init_response.messages, vec![]);
}

#[test]
fn test_manipuate_strategy() {
    let mut deps = mock_dependencies(&[]);
    let _res = instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        InstantiateMsg {
            admin_addr: MOCK_CONTRACT_ADDR.to_string(),
            wormhole_core_bridge_addr: String::from("mock_wormhole_core_bridge"),
            wormhole_token_bridge_addr: String::from("mock_wormhole_token_bridge"),
            cross_chain_outgoing_fee_rate: Decimal::from_ratio(1u128, 1000u128),
            cross_chain_outgoing_fee_collector_addr: String::from("mock_fee_collector"),
        },
    )
    .unwrap();

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

#[test]
fn test_create_position() {
    // Use customized querier enable wasm query. The built-in mock querier
    // doesn't support wasm query yet.
    let mut deps = custom_mock_dependencies();
    let _res = instantiate(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        InstantiateMsg {
            admin_addr: MOCK_CONTRACT_ADDR.to_string(),
            wormhole_core_bridge_addr: String::from("mock_wormhole_core_bridge"),
            wormhole_token_bridge_addr: String::from("mock_wormhole_token_bridge"),
            cross_chain_outgoing_fee_rate: Decimal::from_ratio(1u128, 1000u128),
            cross_chain_outgoing_fee_collector_addr: String::from("mock_fee_collector"),
        },
    )
    .unwrap();

    let _res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        ExecuteMsg::AddStrategy {
            name: "test_strat".to_string(),
            version: "1.0.1".to_string(),
            manager_addr: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g".to_string(),
        },
    );

    let delta_neutral_params_binary = to_binary(&DeltaNeutralParams {
        target_min_collateral_ratio: Decimal::one(),
        target_max_collateral_ratio: Decimal::one(),
        mirror_asset_cw20_addr: MOCK_CONTRACT_ADDR.to_string(),
    })
    .unwrap();

    let execute_res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        ExecuteMsg::CreatePosition {
            strategy: Strategy {
                chain_id: TERRA_CHAIN_ID,
                strategy_id: Uint64::from(0u64),
            },
            data: Some(delta_neutral_params_binary.clone()),
            assets: vec![],
        },
    )
    .unwrap();

    assert_eq!(
        execute_res.messages,
        vec![SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                // Position manager's contract address.
                contract_addr: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g".to_string(),
                msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
                    position: Position {
                        chain_id: TERRA_CHAIN_ID,
                        position_id: Uint128::from(0u128)
                    },
                    action: Action::OpenPosition {
                        data: Some(delta_neutral_params_binary.clone())
                    },
                    assets: vec![],
                })
                .unwrap(),
                funds: vec![],
            })
            .into(),
            gas_limit: None,
            id: 0, // The reply id.
            reply_on: ReplyOn::Never,
        }]
    );

    // Test execute strategy on top of existing positions.
    let execute_res = execute(
        deps.as_mut(),
        mock_env(),
        mock_info(MOCK_CONTRACT_ADDR, &[]),
        ExecuteMsg::ExecuteStrategy {
            position_id: Uint128::zero(),
            action: Action::ClosePosition {
                recipient: Recipient::TerraChain {
                    recipient: MOCK_CONTRACT_ADDR.to_string(),
                },
            },
            assets: vec![],
        },
    )
    .unwrap();

    assert_eq!(
        execute_res.messages,
        vec![SubMsg {
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                // Position manager's contract address.
                contract_addr: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g".to_string(),
                msg: to_binary(&StrategyPositionManagerExecuteMsg::PerformAction {
                    position: Position {
                        chain_id: TERRA_CHAIN_ID,
                        position_id: Uint128::from(0u128)
                    },
                    action: Action::ClosePosition {
                        recipient: Recipient::TerraChain {
                            recipient: MOCK_CONTRACT_ADDR.to_string(),
                        }
                    },
                    assets: vec![],
                })
                .unwrap(),
                funds: vec![],
            })
            .into(),
            gas_limit: None,
            id: 0, // The reply id.
            reply_on: ReplyOn::Never,
        }]
    );
}
