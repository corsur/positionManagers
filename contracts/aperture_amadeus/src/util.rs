use terraswap::asset::AssetInfo;

pub fn get_terraswap_pair_asset_info(cw20_token_addr: &str) -> [AssetInfo; 2] {
    [
        terraswap::asset::AssetInfo::Token {
            contract_addr: cw20_token_addr.to_string(),
        },
        terraswap::asset::AssetInfo::NativeToken {
            denom: String::from("uusd"),
        },
    ]
}
