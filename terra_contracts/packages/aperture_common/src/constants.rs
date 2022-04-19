// When publishing a message via Wormhole, a nonce value must be specified and the nonce gets logged,
// but otherwise plays no role in the lifetime of the message.
// A emitter-specific sequence number gets incremented each time the emitter publishes a message, and
// the sequence number can thus uniquely identify a message from a specific emitter.
pub const WORMHOLE_NONCE: u32 = 0;

// The first byte of an Aperture instruction payload specifies the instruction version.
// The only valid version at this time is 0.
pub const APERTURE_INSTRUCTION_VERSION: u8 = 0;
