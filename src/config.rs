use std::time::Duration;

pub const NETWORK_NAME: &str = "opencoin-mainnet";

pub const BLOCK_TIME_SECONDS: u64 = 120;
pub const BLOCK_TIME: Duration = Duration::from_secs(BLOCK_TIME_SECONDS);

pub const HALVING_INTERVAL_YEARS: u64 = 50;
pub const HALVING_INTERVAL_BLOCKS: u64 = HALVING_INTERVAL_YEARS * 365_250 * 24 * 60 / BLOCK_TIME_SECONDS;

pub const INITIAL_BLOCK_REWARD: u64 = 304;
pub const DECIMAL_PLACES: u8 = 12;
pub const COIN: u64 = 10u64.pow(DECIMAL_PLACES as u32);

pub const TOTAL_SUPPLY: u64 = 8_000_000_000;
pub const PREMINE_AMOUNT: u64 = 20_000_000;
pub const PREMINE_ADDRESS: &str = "OPENCOIN_PREMINE_ADDRESS";

pub const MAX_BLOCK_SIZE: usize = 2_000_000;
pub const MAX_TRANSACTIONS_PER_BLOCK: usize = 5000;

pub const DIFFICULTY_TARGET_SECONDS: u64 = BLOCK_TIME_SECONDS;
pub const DIFFICULTY_WINDOW: u64 = 720;

pub const P2P_PORT: u16 = 9768;
pub const RPC_PORT: u16 = 9769;
pub const MINIMUM_PEERS: usize = 4;
pub const MAXIMUM_PEERS: usize = 50;

pub const VERSION: &str = "0.1.0";
