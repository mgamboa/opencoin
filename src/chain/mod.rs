pub mod block;
pub mod transaction;
pub mod blockchain;
pub mod address;

pub use block::*;
pub use transaction::*;
pub use blockchain::*;
pub use address::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
}

impl Default for NetworkType {
    fn default() -> Self {
        NetworkType::Mainnet
    }
}
