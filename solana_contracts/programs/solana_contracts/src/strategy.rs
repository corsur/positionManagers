//! farm types supported by raydium
use anchor_lang::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, AnchorSerialize, AnchorDeserialize)]
#[repr(u64)]
pub enum PDNStrategy {
    FRANCIUM = 0_u64,
    TUILP = 1_u64,
    ALPACA = 2_u64,
    HOMORA = 3_u64,
    PLACEHOLDER_A = 4,
    PLACEHOLDER_B = 5,
    PLACEHOLDER_C = 6,
    PLACEHOLDER_D = 7,
    PLACEHOLDER_E = 8,
    PLACEHOLDER_F = 9,
    PLACEHOLDER_G = 10,
    PLACEHOLDER_H = 11,
    PLACEHOLDER_I = 12,
    PLACEHOLDER_J = 13,
    PLACEHOLDER_K = 14,
    PLACEHOLDER_L = 15,
    PLACEHOLDER_M = 16,
    PLACEHOLDER_N = 17,
    PLACEHOLDER_O = 18,
    PLACEHOLDER_P = 19,
    PLACEHOLDER_Q = 20,
    PLACEHOLDER_R = 21,
    PLACEHOLDER_S = 22,
    PLACEHOLDER_T = 23,
    PLACEHOLDER_U = 24,
    PLACEHOLDER_V = 25,
    PLACEHOLDER_W = 26,
    PLACEHOLDER_X = 27,
    PLACEHOLDER_Y = 28,
    PLACEHOLDER_Z = 29,
    UNKNOWN = u64::MAX,
}