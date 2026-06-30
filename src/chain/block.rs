use serde::{Deserialize, Serialize};
use super::transaction::{Transaction, TransactionType, TxOutput};
use crate::crypto::keys::PublicKey;
use crate::crypto::stealth::{StealthAddress, OneTimeOutput, EphemeralPublicKey, KeyImage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub height: u64,
    pub timestamp: u64,
    pub previous_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub difficulty_target: u32,
    pub nonce: u64,
    pub extra_nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn genesis() -> Self {
        let null_stealth = StealthAddress {
            spend_pub: PublicKey([0u8; 32]),
            view_pub: PublicKey([0u8; 32]),
        };
        let coinbase_tx = Transaction {
            version: 1,
            tx_type: TransactionType::Coinbase,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                stealth_address: null_stealth,
                one_time_output: OneTimeOutput {
                    ephemeral_pub: EphemeralPublicKey(PublicKey([0u8; 32])),
                    key_image: KeyImage([0u8; 32]),
                    amount_commitment: [0u8; 32],
                },
                amount: 0,
                commitment: None,
                range_proof: None,
                view_key_proof: None,
            }],
            fee: 0,
            timestamp: 0,
            signatures: Vec::new(),
            ring_signature: None,
            memo: Some(String::from("Genesis")),
            contract_code: None,
            contract_address: None,
            contract_fn: None,
        };
        let merkle = crate::crypto::hash::merkle_root(&[coinbase_tx.hash()]);
        Block {
            header: BlockHeader {
                version: 1,
                height: 0,
                timestamp: 1700000000,
                previous_hash: [0u8; 32],
                merkle_root: merkle,
                difficulty_target: 0x1e00ffff,
                nonce: 0,
                extra_nonce: 0,
            },
            transactions: vec![coinbase_tx],
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let encoded = serde_json::to_vec(&self.header).unwrap_or_default();
        crate::crypto::hash::double_sha3_256(&encoded)
    }

    pub fn miner_hash(&self) -> [u8; 32] {
        let mut data = Vec::with_capacity(80);
        data.extend_from_slice(&self.header.version.to_le_bytes());
        data.extend_from_slice(&self.header.height.to_le_bytes());
        data.extend_from_slice(&self.header.timestamp.to_le_bytes());
        data.extend_from_slice(&self.header.previous_hash);
        data.extend_from_slice(&self.header.merkle_root);
        data.extend_from_slice(&self.header.difficulty_target.to_le_bytes());
        data.extend_from_slice(&self.header.nonce.to_le_bytes());
        data.extend_from_slice(&self.header.extra_nonce.to_le_bytes());
        crate::crypto::hash::double_sha3_256(&data)
    }

    pub fn set_nonce(&mut self, nonce: u64, extra_nonce: u64) {
        self.header.nonce = nonce;
        self.header.extra_nonce = extra_nonce;
    }

    pub fn size(&self) -> usize {
        bincode::serialize(self).map(|v| v.len()).unwrap_or(0)
    }
}

pub fn calculate_block_reward(height: u64, premine_remaining: u64) -> u64 {
    if premine_remaining > 0 {
        if height == 0 {
            return crate::config::PREMINE_AMOUNT;
        }
        if premine_remaining >= crate::config::INITIAL_BLOCK_REWARD {
            return crate::config::INITIAL_BLOCK_REWARD;
        }
    }

    let halvings = height / crate::config::HALVING_INTERVAL_BLOCKS;
    let reward = crate::config::INITIAL_BLOCK_REWARD >> halvings;
    if reward < 1 {
        1
    } else {
        reward
    }
}

pub fn is_genesis_block(block: &Block) -> bool {
    block.header.height == 0
}
