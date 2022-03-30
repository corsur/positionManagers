use aperture_common::delta_neutral_position_manager::Context;
use cosmwasm_std::{BlockInfo, Decimal, QuerierWrapper, StdResult};

pub fn get_mirror_asset_oracle_uusd_price_response(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &str,
) -> StdResult<mirror_protocol::oracle::PriceResponse> {
    querier.query_wasm_smart(
        context.mirror_oracle_addr.clone(),
        &mirror_protocol::oracle::QueryMsg::Price {
            base_asset: mirror_asset_cw20_addr.to_string(),
            quote_asset: "uusd".to_string(),
        },
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

fn is_mirror_asset_oracle_price_fresh(
    price_response: &mirror_protocol::oracle::PriceResponse,
    current_block_info: &BlockInfo,
) -> bool {
    // Reference: https://github.com/Mirror-Protocol/mirror-contracts/blob/97cabc2be29635422183c1fb8278f1d5f34d94fc/contracts/mirror_mint/src/querier.rs#L125.
    const PRICE_EXPIRE_TIME: u64 = 60;
    price_response.last_updated_base + PRICE_EXPIRE_TIME >= current_block_info.time.seconds()
}

#[test]
fn test_is_mirror_asset_oracle_price_fresh() {
    use cosmwasm_std::Decimal;
    assert_eq!(
        is_mirror_asset_oracle_price_fresh(
            &mirror_protocol::oracle::PriceResponse {
                rate: Decimal::from_ratio(10u128, 1u128),
                last_updated_base: 12345,
                last_updated_quote: u64::MAX,
            },
            &BlockInfo {
                height: 1,
                time: cosmwasm_std::Timestamp::from_seconds(24689),
                chain_id: String::from("any_chain")
            }
        ),
        false
    );
    assert_eq!(
        is_mirror_asset_oracle_price_fresh(
            &mirror_protocol::oracle::PriceResponse {
                rate: Decimal::from_ratio(10u128, 1u128),
                last_updated_base: 12345,
                last_updated_quote: u64::MAX,
            },
            &BlockInfo {
                height: 1,
                time: cosmwasm_std::Timestamp::from_seconds(12349),
                chain_id: String::from("any_chain")
            }
        ),
        true
    );
}

pub fn get_mirror_asset_fresh_oracle_uusd_rate(
    querier: &QuerierWrapper,
    context: &Context,
    mirror_asset_cw20_addr: &str,
    current_block_info: &BlockInfo,
) -> Option<Decimal> {
    let response =
        get_mirror_asset_oracle_uusd_price_response(querier, context, mirror_asset_cw20_addr)
            .unwrap();
    if is_mirror_asset_oracle_price_fresh(&response, current_block_info) {
        Some(response.rate)
    } else {
        None
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
