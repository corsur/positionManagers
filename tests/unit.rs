use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{
    BankMsg, Binary, Coin, CosmosMsg, HandleResponse, HumanAddr, StdError, StdResult, Uint128, WasmMsg,
};

use amadeus::contract::{handle, init};
use amadeus::msg::{HandleMsg, InitMsg};

fn mock_init_msg() -> InitMsg {
    InitMsg {
        anchor_ust_cw20_addr: HumanAddr::from("anchor_ust_cw20"),
        mirror_collateral_oracle_addr: HumanAddr::from("mirror_collateral_oracle"),
        mirror_lock_addr: HumanAddr::from("mirror_lock"),
        mirror_mint_addr: HumanAddr::from("mirror_mint"),
        mirror_oracle_addr: HumanAddr::from("mirror_orcale"),
        mirror_staking_addr: HumanAddr::from("mirror_staking"),
        spectrum_mirror_farms_addr: HumanAddr::from("spectrum_mirror_farms"),
        spectrum_staker_addr: HumanAddr::from("spectrum_staker"),
        terraswap_factory_addr: HumanAddr::from("terraswap_factory"),
    }
}

fn mock_claim_short_sale_proceeds_and_stake(stake_via_spectrum: bool) -> HandleMsg {
    HandleMsg::ClaimShortSaleProceedsAndStake {
        cdp_idx: Uint128::from(1000u128),
        mirror_asset_amount: Uint128::from(1000000u128),
        stake_via_spectrum,
    }
}

fn mock_close_short_position() -> HandleMsg {
    HandleMsg::CloseShortPosition {
        cdp_idx: Uint128::from(1000u128),
    }
}

fn mock_delta_neutral_invest() -> HandleMsg {
    HandleMsg::DeltaNeutralInvest {
        collateral_asset_amount: Uint128::from(1000000u128),
        collateral_ratio_in_percentage: Uint128::from(200u128),
        mirror_asset_to_mint_cw20_addr: HumanAddr::from("mirror_asset_cw20"),
    }
}

fn mock_do_msg(cosmos_messages: Vec<CosmosMsg>) -> HandleMsg {
    HandleMsg::Do {
        cosmos_messages,
    }
}

fn assert_unauthorized_error(res: StdResult<HandleResponse>) {
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Expecting StdError::Unauthorized."),
    }
}

#[test]
fn authorization() {
    let mut deps = mock_dependencies(/*canonical_length=*/ 30, &[]);
    let env = mock_env(/*sender=*/ "owner", &[]);

    // We can just call .unwrap() to assert this was a success.
    let res = init(&mut deps, env.clone(), mock_init_msg()).unwrap();
    assert_eq!(0, res.messages.len());

    let unauthorized_env = mock_env(/*sender=*/ "anyone", &[]);
    assert_unauthorized_error(handle(&mut deps, unauthorized_env.clone(), mock_claim_short_sale_proceeds_and_stake(false)));
    assert_unauthorized_error(handle(&mut deps, unauthorized_env.clone(), mock_close_short_position()));
    assert_unauthorized_error(handle(&mut deps, unauthorized_env.clone(), mock_delta_neutral_invest()));
    assert_unauthorized_error(handle(&mut deps, unauthorized_env, mock_do_msg(vec![])));

    // Assert that owner can successfully execute an empty Do call.
    let _res = handle(&mut deps, env, mock_do_msg(vec![])).unwrap();
}

#[test]
fn execute_do() {
    let mut deps = mock_dependencies(/*canonical_length=*/ 30, &[]);
    let env = mock_env(/*sender=*/ "owner", &[]);
    let _res = init(&mut deps, env.clone(), mock_init_msg()).unwrap();

    let messages = vec![CosmosMsg::Bank(BankMsg::Send {
        from_address: env.contract.address.clone(),
        to_address: env.message.sender.clone(),
        amount: vec![
            Coin {
                amount: Uint128::from(30u128),
                denom: String::from("moon"),
            },
        ],
    }), CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: HumanAddr::from("some_contract"),
        msg: Binary::from_base64("TW9vbg==").unwrap(),
        send: vec![],
    })];

    let res = handle(&mut deps, env, mock_do_msg(messages.clone())).unwrap();
    assert_eq!(res.messages, messages);
}
