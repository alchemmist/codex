mod rpc;
mod types;
mod validation;

pub use rpc::*;
pub use types::*;
pub use validation::ProtocolError;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_TEXT_BYTES: usize = 8_000;
