use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::config;
use crate::consensus::pow::check_pow;
use crate::chain::block::{Block, calculate_block_reward};
use crate::chain::transaction::{TransactionType, TxOutput};
use crate::crypto::hash::merkle_root;
use crate::crypto::stealth::StealthAddress;
use crate::storage::db::Storage;
use crate::vm::wasm::{WasmRuntime, ContractContext};

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
    pub utxo_set: HashMap<String, TxOutput>,
    pub key_images: HashSet<[u8; 32]>,
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
            key_images: HashSet::new(),
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
        let mut key_images = HashSet::new();
        for block in &blocks {
            for tx in &block.transactions {
                if tx.tx_type != TransactionType::Coinbase {
                    for (i, output) in tx.outputs.iter().enumerate() {
                        let utxo_key = format!("{}:{}", hex::encode(tx.hash()), i);
                        utxo_set.insert(utxo_key, output.clone());
                    }
                }
                for input in &tx.inputs {
                    let spent_key = format!("{}:{}", hex::encode(input.outpoint.tx_hash), input.outpoint.index);
                    utxo_set.remove(&spent_key);
                    key_images.insert(input.key_image.0);
                }
            }
        }
        let current_hash = blocks.last().map(|b| b.hash()).unwrap_or([0u8; 32]);
        Ok(Some(Blockchain {
            blocks,
            utxo_set,
            key_images,
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
            if tx.tx_type == TransactionType::Transfer {
                if let Err(e) = tx.verify_signatures() {
                    return Err(e);
                }
            }
            if tx.tx_type == TransactionType::Private {
                if let Err(e) = tx.verify_signatures() {
                    return Err(e);
                }
                for output in &tx.outputs {
                    if let Some(ref rp) = output.range_proof {
                        if !rp.verify() {
                            return Err("Private tx range proof verification failed");
                        }
                    }
                }
                for input in &tx.inputs {
                    if self.key_images.contains(&input.key_image.0) {
                        return Err("Double spend: key image already used");
                    }
                }
            }
            if tx.tx_type == TransactionType::ContractDeploy || tx.tx_type == TransactionType::ContractCall {
                if let Err(e) = tx.verify_signatures() {
                    return Err(e);
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
                for (i, output) in tx.outputs.iter().enumerate() {
                    let utxo_key = format!("{}:{}", hex::encode(tx.hash()), i);
                    self.utxo_set.insert(utxo_key, output.clone());
                }
            }
            for input in &tx.inputs {
                let spent_key = format!("{}:{}", hex::encode(input.outpoint.tx_hash), input.outpoint.index);
                self.utxo_set.remove(&spent_key);
                if tx.tx_type == TransactionType::Private {
                    self.key_images.insert(input.key_image.0);
                }
            }
        }

        struct ContractUpdate {
            addr: [u8; 32],
            code: Option<Vec<u8>>,
            persist: HashMap<String, Vec<u8>>,
        }
        let mut contract_updates: Vec<ContractUpdate> = Vec::new();
        for tx in &block.transactions {
            if tx.tx_type == TransactionType::ContractDeploy {
                if let Some(ref code) = tx.contract_code {
                    let addr = crate::vm::wasm::contract_address(&tx.hash(), block.header.height);
                    let persist = HashMap::new();
                    let mut ctx = ContractContext {
                        caller: [0u8; 32],
                        block_height: block.header.height,
                        contract_address: addr,
                        events: Vec::new(),
                        persist,
                    };
                    let runtime = WasmRuntime::new().map_err(|_| "WASM runtime init failed")?;
                    runtime.deploy(code, tx.contract_fn.as_ref().map(|s| s.as_bytes()).unwrap_or(b""), &mut ctx).map_err(|_| "Contract deploy failed")?;
                    contract_updates.push(ContractUpdate { addr, code: Some(code.clone()), persist: ctx.persist });
                }
            }
            if tx.tx_type == TransactionType::ContractCall {
                if let Some(addr) = tx.contract_address {
                    let code = if let Some(ref storage) = self.storage {
                        if let Ok(s) = storage.lock() {
                            s.load_contract_code(&addr).ok().flatten()
                        } else { None }
                    } else { None };
                    let code = code.ok_or("Contract code not found")?;
                    let persist = HashMap::new();
                    let mut ctx = ContractContext {
                        caller: [0u8; 32],
                        block_height: block.header.height,
                        contract_address: addr,
                        events: Vec::new(),
                        persist,
                    };
                    let fn_args = tx.contract_fn.as_ref().map(|f| {
                        let mut b = f.as_bytes().to_vec();
                        b.push(0);
                        b
                    }).unwrap_or_default();
                    let runtime = WasmRuntime::new().map_err(|_| "WASM runtime init failed")?;
                    runtime.call(&code, &fn_args, &mut ctx).map_err(|_| "Contract call failed")?;
                    contract_updates.push(ContractUpdate { addr, code: None, persist: ctx.persist });
                }
            }
        }

        if let Some(ref storage) = self.storage {
            if let Ok(s) = storage.lock() {
                let _ = s.save_block(&block);
                let _ = s.save_blockchain_state(&self.state);
                for update in &contract_updates {
                    if let Some(ref c) = update.code {
                        let _ = s.save_contract_code(&update.addr, c);
                    }
                    if !update.persist.is_empty() {
                        let serialized = serde_json::to_vec(
                            &update.persist.iter().map(|(k, v)| (k.clone(), String::from_utf8_lossy(v).to_string())).collect::<HashMap<_, _>>()
                        ).unwrap_or_default();
                        let _ = s.save_contract_state(&update.addr, ":all", &serialized);
                    }
                }
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

    pub fn try_accept_chain(&mut self, new_blocks: Vec<Block>) -> Result<(), &'static str> {
        if new_blocks.is_empty() {
            return Ok(());
        }
        let new_last = new_blocks.last().unwrap();
        let new_work: u128 = new_blocks.iter()
            .map(|b| 1u128 << (64 - b.header.difficulty_target))
            .sum();
        if new_work <= self.state.total_work && new_last.header.height <= self.state.height {
            return Err("Chain has less work than current chain");
        }
        for block in &new_blocks {
            if block.header.height == 0 { continue; }
            if self.get_block_by_hash(&block.header.previous_hash).is_none() &&
               !new_blocks.iter().any(|b| b.hash() == block.header.previous_hash) {
                return Err("Cannot find previous block in chain");
            }
            let target = crate::consensus::difficulty::compact_to_target(block.header.difficulty_target);
            if !check_pow(block, target) {
                return Err("Block does not meet difficulty target");
            }
        }

        let common_height = new_blocks.first().map(|b| {
            if b.header.height == 0 { 0 }
            else { b.header.height - 1 }
        }).unwrap_or(0);

        let old_tip_height = self.state.height;
        let mut reorg_blocks: Vec<Block> = Vec::new();
        if common_height < old_tip_height {
            for h in (common_height + 1)..=old_tip_height {
                if let Some(b) = self.get_block(h).cloned() {
                    reorg_blocks.push(b);
                }
            }
        }

        for block in &new_blocks {
            let height = block.header.height;
            if height <= old_tip_height {
                if let Some(existing) = self.get_block(height) {
                    if existing.hash() != block.hash() {
                        let idx = self.blocks.iter().position(|b| b.header.height == height).unwrap();
                        self.blocks.remove(idx);
                        self.blocks.push(block.clone());
                    }
                }
            } else {
                self.blocks.push(block.clone());
            }
        }

        self.utxo_set.clear();
        let mut sorted: Vec<&Block> = self.blocks.iter().filter(|b| b.header.height > 0).collect();
        sorted.sort_by_key(|b| b.header.height);
        for block in &sorted {
            for tx in &block.transactions {
                if tx.tx_type != TransactionType::Coinbase {
                    for (i, output) in tx.outputs.iter().enumerate() {
                        let utxo_key = format!("{}:{}", hex::encode(tx.hash()), i);
                        self.utxo_set.insert(utxo_key, output.clone());
                    }
                }
                for input in &tx.inputs {
                    let spent_key = format!("{}:{}", hex::encode(input.outpoint.tx_hash), input.outpoint.index);
                    self.utxo_set.remove(&spent_key);
                }
            }
        }

        self.state.current_hash = new_last.hash();
        self.state.height = new_last.header.height;
        self.state.total_work = self.blocks.iter()
            .filter(|b| b.header.height > 0)
            .map(|b| 1u128 << (64 - b.header.difficulty_target))
            .sum();
        self.state.circulating_supply = self.blocks.iter()
            .flat_map(|b| &b.transactions)
            .filter(|t| t.tx_type == TransactionType::Coinbase)
            .map(|t| t.total_output())
            .sum();

        Ok(())
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
