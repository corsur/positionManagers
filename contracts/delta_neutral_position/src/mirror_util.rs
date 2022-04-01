use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{Addr, Decimal, QuerierWrapper, StdResult};

pub fn get_mirror_asset_oracle_uusd_price_response(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
) -> StdResult<tefi_oracle::hub::PriceResponse> {
    tefi_oracle::querier::query_asset_price(
        querier,
        &context.mirror_oracle_addr,
        mirror_asset_cw20_addr,
        None,
    )
}

pub fn get_mirror_asset_config_response(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &str,
) -> StdResult<mirror_protocol::mint::AssetConfigResponse> {
    querier.query_wasm_smart(
        context.mirror_mint_addr.clone(),
        &mirror_protocol::mint::QueryMsg::AssetConfig {
            asset_token: mirror_asset_cw20_addr.to_string(),
        },
    )
}

pub fn get_mirror_asset_fresh_oracle_uusd_rate(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &Addr,
) -> Option<Decimal> {
    // Reference: https://github.com/Mirror-Protocol/mirror-contracts/blob/4d0b026a4505b806113b83f7253cf67b187ba292/contracts/mirror_mint/src/querier.rs#L89
    const PRICE_EXPIRE_TIME: u64 = 60;
    match tefi_oracle::querier::query_asset_price(
        querier,
        &context.mirror_oracle_addr,
        mirror_asset_cw20_addr,
        Some(PRICE_EXPIRE_TIME),
    ) {
        Ok(price_response) => Some(price_response.rate),
        _ => None,
    }
}

pub fn is_mirror_asset_delisted(
    asset_config_response: &mirror_protocol::mint::AssetConfigResponse,
) -> bool {
    asset_config_response.end_price.is_some()
}

#[test]
fn test_is_mirror_asset_delisted() {
    use cosmwasm_std::Decimal;
    assert_eq!(
        is_mirror_asset_delisted(&mirror_protocol::mint::AssetConfigResponse {
            token: String::from("token"),
            auction_discount: Decimal::zero(),
            min_collateral_ratio: Decimal::from_ratio(15u128, 10u128),
            end_price: None,
            ipo_params: None,
        }),
        false
    );
    assert_eq!(
        is_mirror_asset_delisted(&mirror_protocol::mint::AssetConfigResponse {
            token: String::from("token"),
            auction_discount: Decimal::zero(),
            min_collateral_ratio: Decimal::from_ratio(15u128, 10u128),
            end_price: Some(Decimal::one()),
            ipo_params: None,
        }),
        true
    );
}
