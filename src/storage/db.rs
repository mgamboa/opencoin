use sled::Db;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::chain::block::Block;
use crate::chain::transaction::Transaction;
use crate::chain::blockchain::BlockchainState;
use crate::wallet::Wallet;

pub struct Storage {
    db: Db,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let db = sled::open(path)?;
        Ok(Storage { db })
    }

    pub fn save_block(&self, block: &Block) -> Result<(), Box<dyn std::error::Error>> {
        let key = format!("block:{}", block.header.height);
        let value = bincode::serialize(block)?;
        self.db.insert(key.as_bytes(), value)?;
        self.db.insert(format!("hash:{}", hex::encode(block.hash())).as_bytes(), &block.header.height.to_le_bytes())?;
        Ok(())
    }

    pub fn get_block(&self, height: u64) -> Result<Option<Block>, Box<dyn std::error::Error>> {
        let key = format!("block:{}", height);
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let block: Block = bincode::deserialize(&data)?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    pub fn get_block_by_hash(&self, hash: &[u8; 32]) -> Result<Option<Block>, Box<dyn std::error::Error>> {
        let key = format!("hash:{}", hex::encode(hash));
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let height = u64::from_le_bytes(data.as_ref().try_into()?);
                self.get_block(height)
            }
            None => Ok(None),
        }
    }

    pub fn save_blockchain_state(&self, state: &BlockchainState) -> Result<(), Box<dyn std::error::Error>> {
        let value = bincode::serialize(state)?;
        self.db.insert(b"blockchain_state", value)?;
        Ok(())
    }

    pub fn get_blockchain_state(&self) -> Result<Option<BlockchainState>, Box<dyn std::error::Error>> {
        match self.db.get(b"blockchain_state")? {
            Some(data) => {
                let state: BlockchainState = bincode::deserialize(&data)?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    pub fn save_transaction(&self, tx: &Transaction) -> Result<(), Box<dyn std::error::Error>> {
        let key = format!("tx:{}", hex::encode(tx.hash()));
        let value = bincode::serialize(tx)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_transaction(&self, tx_hash: &[u8; 32]) -> Result<Option<Transaction>, Box<dyn std::error::Error>> {
        let key = format!("tx:{}", hex::encode(tx_hash));
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let tx: Transaction = bincode::deserialize(&data)?;
                Ok(Some(tx))
            }
            None => Ok(None),
        }
    }

    pub fn height(&self) -> Result<u64, Box<dyn std::error::Error>> {
        match self.get_blockchain_state()? {
            Some(state) => Ok(state.height),
            None => Ok(0),
        }
    }

    pub fn get_blocks_since(&self, from_height: u64) -> Result<Vec<Block>, Box<dyn std::error::Error>> {
        let state = match self.get_blockchain_state()? {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };
        let mut blocks = Vec::with_capacity((state.height - from_height + 1) as usize);
        for h in from_height..=state.height {
            if let Some(block) = self.get_block(h)? {
                blocks.push(block);
            }
        }
        Ok(blocks)
    }

    pub fn get_all_blocks(&self) -> Result<Vec<Block>, Box<dyn std::error::Error>> {
        self.get_blocks_since(0)
    }

    pub fn save_wallet(&self, wallet: &Wallet) -> Result<(), Box<dyn std::error::Error>> {
        let value = bincode::serialize(wallet)?;
        self.db.insert(b"wallet", value)?;
        Ok(())
    }

    pub fn load_wallet(&self) -> Result<Option<Wallet>, Box<dyn std::error::Error>> {
        match self.db.get(b"wallet")? {
            Some(data) => {
                let wallet: Wallet = bincode::deserialize(&data)?;
                Ok(Some(wallet))
            }
            None => Ok(None),
        }
    }

    pub fn flush(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.db.flush()?;
        Ok(())
    }
}
