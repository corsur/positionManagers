//! farm types supported by raydium
use anchor_lang::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, AnchorSerialize, AnchorDeserialize)]
#[repr(u64)]
pub enum PDNStrategy {
    FRANCIUM_RAYDIUM = 0_u64,
    TUILP_RAYDIUM = 1_u64,
    FRANCIUM_ORCA = 2_u64,
    TUILP_ORCA = 3_u64,
    ALPACA = 4_u64,
    HOMORA = 5_u64,
    PLACEHOLDER_A = 6,
    PLACEHOLDER_B = 7,
    PLACEHOLDER_C = 8,
    PLACEHOLDER_D = 9,
    PLACEHOLDER_E = 10,
    PLACEHOLDER_F = 11,
    PLACEHOLDER_G = 12,
    PLACEHOLDER_H = 13,
    PLACEHOLDER_I = 14,
    PLACEHOLDER_J = 15,
    PLACEHOLDER_K = 16,
    PLACEHOLDER_L = 17,
    PLACEHOLDER_M = 18,
    PLACEHOLDER_N = 19,
    PLACEHOLDER_O = 20,
    PLACEHOLDER_P = 21,
    PLACEHOLDER_Q = 22,
    PLACEHOLDER_R = 23,
    PLACEHOLDER_S = 24,
    PLACEHOLDER_T = 25,
    PLACEHOLDER_U = 26,
    PLACEHOLDER_V = 27,
    PLACEHOLDER_W = 28,
    PLACEHOLDER_X = 29,
    PLACEHOLDER_Y = 30,
    PLACEHOLDER_Z = 31,
    UNKNOWN = u64::MAX,
}