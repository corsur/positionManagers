
#[derive(
    AnchorSerialize, AnchorDeserialize, FromPrimitive, ToPrimitive, Copy, Clone, PartialEq, Eq,
)]
pub struct Config {
    pub u32 crossChainFeeBPS; // Cross-chain fee in bpq.
    pub Pubkey feeSink; // Fee collecting address.
}