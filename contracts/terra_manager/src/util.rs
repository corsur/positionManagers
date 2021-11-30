use aperture_common::common::StrategyType;
use cw_storage_plus::U32Key;

pub fn get_strategy_key(strategy_type: &StrategyType) -> U32Key {
    U32Key::from(match strategy_type {
        StrategyType::DeltaNeutral(_) => 0,
    })
}
