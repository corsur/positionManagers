use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{
    coins, from_binary, HandleResponse, HandleResult, HumanAddr, StdError,
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

fn mock_do_msg() -> HandleMsg {
    HandleMsg::Do {
        cosmos_messages: vec![],
    }
}

#[test]
fn authorization() {
    let mut deps = mock_dependencies(/*canonical_length=*/ 30, &[]);
    let env = mock_env(/*sender=*/ "owner", &[]);

    // We can just call .unwrap() to assert this was a success.
    let res = init(&mut deps, env, mock_init_msg()).unwrap();
    assert_eq!(0, res.messages.len());

    let unauthorized_env = mock_env(/*sender=*/ "anyone", &[]);
    let res = handle(&mut deps, unauthorized_env, mock_do_msg());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Must return StdError::Unauthorized."),
    }
}
