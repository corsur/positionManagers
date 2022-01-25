use aperture_common::delta_neutral_position::{PositionActionInfo, TargetCollateralRatioRange};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

pub const MANAGER: Item<Addr> = Item::new("manager");
pub const POSITION_OPEN_INFO: Item<PositionActionInfo> = Item::new("position_open_info");
pub const POSITION_CLOSE_INFO: Item<PositionActionInfo> = Item::new("position_close_info");
pub const CDP_IDX: Item<Uint128> = Item::new("cdp_idx");
pub const MIRROR_ASSET_CW20_ADDR: Item<Addr> = Item::new("mirror_asset_cw20_addr");
pub const TARGET_COLLATERAL_RATIO_RANGE: Item<TargetCollateralRatioRange> =
    Item::new("target_collateral_ratio_range");
pub const LAST_FEE_COLLECTION_POSITION_UUSD_VALUE: Item<Uint128> =
    Item::new("last_fee_collection_position_uusd_value");
