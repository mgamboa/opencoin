use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config;
use crate::consensus::pow::check_pow;
use crate::chain::block::{is_genesis_block, Block, calculate_block_reward};
use crate::chain::transaction::{Transaction, TransactionType};
use crate::crypto::hash::merkle_root;
use crate::crypto::stealth::StealthAddress;
use crate::storage::db::Storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainState {
    pub height: u64,
    pub current_hash: [u8; 32],
    pub total_work: u128,
    pub premine_remaining: u64,
    pub circulating_supply: u64,
}

pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub utxo_set: HashMap<String, Transaction>,
    pub state: BlockchainState,
    pub premine_address: StealthAddress,
    pub storage: Option<Arc<std::sync::Mutex<Storage>>>,
}

impl Blockchain {
    pub fn new(premine_address: StealthAddress) -> Self {
        let genesis = Block::genesis();

        let mut bc = Blockchain {
            blocks: vec![genesis],
            utxo_set: HashMap::new(),
            state: BlockchainState {
                height: 0,
                current_hash: [0u8; 32],
                total_work: 0,
                premine_remaining: config::PREMINE_AMOUNT,
                circulating_supply: 0,
            },
            premine_address,
            storage: None,
        };

        bc.state.current_hash = bc.blocks[0].hash();
        bc.add_premine();
        bc
    }

    pub fn set_storage(&mut self, storage: Arc<std::sync::Mutex<Storage>>) {
        self.storage = Some(storage);
    }

    fn add_premine(&mut self) {
        if self.state.premine_remaining > 0 {
            self.state.circulating_supply += self.state.premine_remaining;
            self.state.premine_remaining = 0;
        }
    }

    pub fn load_from_storage(storage: &Storage, premine_address: StealthAddress) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let state = match storage.get_blockchain_state()? {
            Some(s) => s,
            None => return Ok(None),
        };
        let mut blocks = storage.get_all_blocks()?;
        if blocks.is_empty() {
            return Ok(None);
        }
        blocks.sort_by_key(|b| b.header.height);
        let has_genesis = blocks.first().map_or(false, |b| b.header.height == 0);
        if !has_genesis {
            blocks.insert(0, Block::genesis());
        }
        let mut utxo_set = HashMap::new();
        for block in &blocks {
            for tx in &block.transactions {
                if tx.tx_type != TransactionType::Coinbase {
                    utxo_set.insert(hex::encode(tx.hash()), tx.clone());
                }
            }
        }
        let current_hash = blocks.last().map(|b| b.hash()).unwrap_or([0u8; 32]);
        Ok(Some(Blockchain {
            blocks,
            utxo_set,
            state: BlockchainState {
                height: state.height,
                current_hash,
                total_work: state.total_work,
                premine_remaining: state.premine_remaining,
                circulating_supply: state.circulating_supply,
            },
            premine_address,
            storage: None,
        }))
    }

    pub fn add_block(&mut self, block: Block) -> Result<(), &'static str> {
        if block.header.height != self.state.height + 1 {
            return Err("Invalid block height");
        }
        if block.header.previous_hash != self.state.current_hash {
            return Err("Invalid previous hash");
        }

        let target = crate::consensus::difficulty::compact_to_target(block.header.difficulty_target);
        if !check_pow(&block, target) {
            return Err("Block does not meet difficulty target");
        }

        let mut coinbase_found = false;
        for tx in &block.transactions {
            if tx.tx_type == TransactionType::Coinbase {
                if coinbase_found {
                    return Err("Multiple coinbase transactions");
                }
                coinbase_found = true;
                if tx.total_output() > calculate_block_reward(block.header.height, self.state.premine_remaining) {
                    return Err("Coinbase exceeds reward");
                }
            }
        }
        if !coinbase_found {
            return Err("Missing coinbase transaction");
        }

        let mut tx_hashes = Vec::with_capacity(block.transactions.len());
        for tx in &block.transactions {
            tx_hashes.push(tx.hash());
        }
        let computed_merkle = merkle_root(&tx_hashes);
        if computed_merkle != block.header.merkle_root {
            return Err("Invalid merkle root");
        }

        self.state.height = block.header.height;
        self.state.current_hash = block.hash();
        self.state.total_work += self.state.total_work.saturating_add(1u128 << (64 - block.header.difficulty_target));
        self.state.circulating_supply += block.transactions.iter()
            .filter(|t| t.tx_type == TransactionType::Coinbase)
            .map(|t| t.total_output())
            .sum::<u64>();

        self.blocks.push(block.clone());

        for tx in &block.transactions {
            if tx.tx_type != TransactionType::Coinbase {
                self.utxo_set.insert(hex::encode(tx.hash()), tx.clone());
            }
        }

        if let Some(ref storage) = self.storage {
            if let Ok(s) = storage.lock() {
                let _ = s.save_block(&block);
                let _ = s.save_blockchain_state(&self.state);
                let _ = s.flush();
            }
        }

        Ok(())
    }

    pub fn get_block(&self, height: u64) -> Option<&Block> {
        self.blocks.iter().find(|b| b.header.height == height)
    }

    pub fn get_block_by_hash(&self, hash: &[u8; 32]) -> Option<&Block> {
        self.blocks.iter().find(|b| b.hash() == *hash)
    }

    pub fn is_valid(&self) -> bool {
        let mut sorted: Vec<&Block> = self.blocks.iter().collect();
        sorted.sort_by_key(|b| b.header.height);
        for i in 1..sorted.len() {
            let prev = sorted[i - 1];
            let curr = sorted[i];
            if curr.header.previous_hash != prev.hash() {
                return false;
            }
            if curr.header.height != prev.header.height + 1 {
                return false;
            }
        }
        true
    }
}
