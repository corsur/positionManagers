use cosmwasm_std::{coins, from_binary, HandleResponse, HandleResult, HumanAddr, InitResponse, StdError};
use cosmwasm_std::testing::{mock_dependencies, mock_env};

use amadeus::contract::{init};
use amadeus::msg::{HandleMsg, InitMsg};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(/*canonical_length=*/30, &[]);
    let env = mock_env(/*sender=*/"creator", &[]);
    let mock_init_msg = InitMsg {
        anchor_ust_cw20_addr: HumanAddr::from("anchor_ust_cw20"),
        mirror_collateral_oracle_addr: HumanAddr::from("mirror_collateral_oracle"),
        mirror_lock_addr: HumanAddr::from("mirror_lock"),
        mirror_mint_addr: HumanAddr::from("mirror_mint"),
        mirror_oracle_addr: HumanAddr::from("mirror_orcale"),
        mirror_staking_addr: HumanAddr::from("mirror_staking"),
        spectrum_mirror_farms_addr: HumanAddr::from("spectrum_mirror_farms"),
        spectrum_staker_addr: HumanAddr::from("spectrum_staker"),
        terraswap_factory_addr: HumanAddr::from("terraswap_factory"),
    };

    // We can just call .unwrap() to assert this was a success.
    let res: InitResponse = init(&mut deps, env, mock_init_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

