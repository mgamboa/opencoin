pub mod config;
pub mod chain;
pub mod crypto;
pub mod consensus;
pub mod storage;
pub mod p2p;
pub mod wallet;
pub mod rpc;
pub mod vm;
pub mod util;
pub mod pool;
pub mod light_client;

pub const OPENCOIN_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u32 = 1;
