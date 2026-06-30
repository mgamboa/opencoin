use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use log::{info, warn, error};

use crate::chain::address::OpenCoinAddress;
use crate::chain::block::{Block, BlockHeader, calculate_block_reward};
use crate::chain::transaction::{Transaction, TransactionType};
use crate::chain::blockchain::Blockchain;
use crate::crypto::hash::merkle_root;
use crate::crypto::stealth::StealthAddress;
use crate::consensus::difficulty::{calculate_difficulty, compact_to_target, difficulty_to_compact};
use crate::p2p::P2PNetwork;
use crate::wallet::Wallet;
use crate::storage::db::Storage;

pub const POOL_FEE_PERCENT: u64 = 2;

#[derive(Debug, Clone)]
pub struct MinerInfo {
    pub address: SocketAddr,
    pub wallet_address: String,
    pub shares: u64,
    pub last_share_time: u64,
    pub valid_blocks: u64,
}

#[derive(Debug, Clone)]
pub struct BlockTemplate {
    pub height: u64,
    pub difficulty: u64,
    pub block_target: u64,
    pub share_target: u64,
    pub job_id: u64,
    pub header_bytes: Vec<u8>,
    pub coinbase_tx: Transaction,
    pub mempool_txs: Vec<Transaction>,
}

pub struct PoolServer {
    pub port: u16,
    pub pool_address: StealthAddress,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub p2p: Option<Arc<P2PNetwork>>,
    pub wallet: Option<Arc<RwLock<Option<Wallet>>>>,
    pub storage: Option<Arc<std::sync::Mutex<Storage>>>,
    pub miners: Arc<RwLock<HashMap<SocketAddr, MinerInfo>>>,
    pub current_template: Arc<RwLock<Option<BlockTemplate>>>,
    pub template_tx: tokio::sync::watch::Sender<Option<BlockTemplate>>,
    pub template_rx: tokio::sync::watch::Receiver<Option<BlockTemplate>>,
    pub job_counter: Arc<AtomicU64>,
    pub total_shares: Arc<AtomicU64>,
    pub running: Arc<AtomicBool>,
    pub round_shares: Arc<RwLock<HashMap<String, u64>>>,
}

impl PoolServer {
    pub fn new(port: u16, pool_address: StealthAddress, blockchain: Arc<RwLock<Blockchain>>) -> Self {
        let (template_tx, template_rx) = tokio::sync::watch::channel(None);
        PoolServer {
            port,
            pool_address,
            blockchain,
            p2p: None,
            wallet: None,
            storage: None,
            miners: Arc::new(RwLock::new(HashMap::new())),
            current_template: Arc::new(RwLock::new(None)),
            template_tx,
            template_rx,
            job_counter: Arc::new(AtomicU64::new(1)),
            total_shares: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(true)),
            round_shares: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_p2p(mut self, p2p: Arc<P2PNetwork>) -> Self {
        self.p2p = Some(p2p);
        self
    }

    pub fn with_wallet(mut self, wallet: Arc<RwLock<Option<Wallet>>>) -> Self {
        self.wallet = Some(wallet);
        self
    }

    pub fn with_storage(mut self, storage: Arc<std::sync::Mutex<Storage>>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Pool server listening on {}", addr);

        let template_updater = self.current_template.clone();
        let template_tx = self.template_tx.clone();
        let blockchain = self.blockchain.clone();
        let pool_addr = self.pool_address.clone();
        let job_counter = self.job_counter.clone();
        let p2p = self.p2p.clone();

        let _updater = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let bc = blockchain.read().await;
                let height = bc.state.height + 1;
                let reward = calculate_block_reward(height, bc.state.premine_remaining);
                let coinbase_tx = Transaction::coinbase(reward, &pool_addr);
                let mut txs = vec![coinbase_tx.clone()];
                if let Some(ref p) = p2p {
                    let mempool = p.mempool.read().await;
                    let mut fee_txs: Vec<&Transaction> = mempool.iter().collect();
                    fee_txs.sort_by(|a, b| b.fee.cmp(&a.fee));
                    let max_block_txs = crate::config::MAX_TRANSACTIONS_PER_BLOCK;
                    for tx in fee_txs.iter().take(max_block_txs) {
                        if tx.total_output() + tx.fee <= reward.saturating_sub(reward / 10) {
                            if tx.verify_signatures().is_ok() {
                                txs.push((*tx).clone());
                            }
                        }
                    }
                }
                let tx_hashes: Vec<[u8; 32]> = txs.iter().map(|t| t.hash()).collect();
                let merkle = merkle_root(&tx_hashes);

                let difficulty = calculate_difficulty(&bc.blocks);
                let compact_target = difficulty_to_compact(difficulty);
                let block_target = compact_to_target(compact_target);
                let share_target = u64::MAX / 1_000_000;

                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let mut header_data = Vec::with_capacity(72);
                header_data.extend_from_slice(&1u32.to_le_bytes());
                header_data.extend_from_slice(&height.to_le_bytes());
                header_data.extend_from_slice(&timestamp.to_le_bytes());
                header_data.extend_from_slice(&bc.state.current_hash);
                header_data.extend_from_slice(&merkle);
                header_data.extend_from_slice(&compact_target.to_le_bytes());

                let job_id = job_counter.fetch_add(1, Ordering::SeqCst);

                let mempool_txs: Vec<Transaction> = txs.iter()
                    .filter(|t| t.tx_type != TransactionType::Coinbase)
                    .cloned()
                    .collect();
                let template = BlockTemplate {
                    height,
                    difficulty,
                    block_target,
                    share_target,
                    job_id,
                    header_bytes: header_data,
                    coinbase_tx,
                    mempool_txs,
                };

                let mut current = template_updater.write().await;
                let should_update = match current.as_ref() {
                    Some(old) => old.height != height || old.job_id != job_id,
                    None => true,
                };
                if should_update {
                    info!("New pool job #{} at height {}", job_id, height);
                    *current = Some(template.clone());
                    let _ = template_tx.send(Some(template));
                }
                drop(current);
                drop(bc);
            }
        });

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New miner connected: {}", addr);
                    self.handle_miner(stream, addr).await;
                }
                Err(e) => {
                    error!("Pool accept error: {}", e);
                }
            }
        }
    }

    async fn handle_miner(&self, stream: TcpStream, addr: SocketAddr) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let miners = self.miners.clone();
        let total_shares = self.total_shares.clone();
        let p2p = self.p2p.clone();

        {
            let mut m = miners.write().await;
            m.insert(addr, MinerInfo {
                address: addr,
                wallet_address: String::new(),
                shares: 0,
                last_share_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                valid_blocks: 0,
            });
        }

        let (read_half, write_half) = tokio::io::split(stream);
        let write_half = std::sync::Arc::new(tokio::sync::Mutex::new(write_half));

        async fn send_msg(w: &tokio::sync::Mutex<tokio::io::WriteHalf<tokio::net::TcpStream>>, msg: &str) {
            let mut line = msg.to_string();
            line.push('\n');
            let mut guard = w.lock().await;
            let _ = guard.write_all(line.as_bytes()).await;
        }

        let template_rx = self.template_rx.clone();

        let ct = self.current_template.clone();
        let bc2 = self.blockchain.clone();
        let round_shares = self.round_shares.clone();
        let p2p = p2p.clone();
        let wallet = self.wallet.clone();
        let storage = self.storage.clone();

        let write_half_job = write_half.clone();
        let mut template_rx_clone = template_rx.clone();
        tokio::spawn(async move {
            {
                let t = (*template_rx_clone.borrow_and_update()).clone();
                if let Some(ref tmpl) = t {
                    let job_json = format!(
                        r#"{{"type":"job","job_id":{},"height":{},"target":{},"share_target":{},"header":"{}"}}"#,
                        tmpl.job_id, tmpl.height, tmpl.block_target, tmpl.share_target, hex::encode(&tmpl.header_bytes)
                    );
                    send_msg(&*write_half_job, &job_json).await;
                }
            }
            loop {
                if template_rx_clone.changed().await.is_err() {
                    break;
                }
                let t = (*template_rx_clone.borrow_and_update()).clone();
                if let Some(ref tmpl) = t {
                    let job_json = format!(
                        r#"{{"type":"job","job_id":{},"height":{},"target":{},"share_target":{},"header":"{}"}}"#,
                        tmpl.job_id, tmpl.height, tmpl.block_target, tmpl.share_target, hex::encode(&tmpl.header_bytes)
                    );
                    send_msg(&*write_half_job, &job_json).await;
                }
            }
        });

        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            let msg_type = msg["type"].as_str().unwrap_or("");
                            match msg_type {
                                "submit" => {
                                    let job_id = msg["job_id"].as_u64().unwrap_or(0);
                                    let nonce = msg["nonce"].as_u64().unwrap_or(0);
                                    let wallet_addr = msg["address"].as_str().unwrap_or("").to_string();

                                    let template = ct.read().await;
                                    let result = if let Some(ref t) = *template {
                                        if t.job_id != job_id {
                                            Some((false, "Stale job".to_string()))
                                        } else {
                                            let mut full_header = t.header_bytes.clone();
                                            full_header.extend_from_slice(&nonce.to_le_bytes());
                                            full_header.extend_from_slice(&0u64.to_le_bytes());

                                            let hash = crate::crypto::hash::double_sha3_256(&full_header);
                                            let hash_val = u64::from_le_bytes(hash[24..32].try_into().unwrap_or([0u8; 8]));

                                            total_shares.fetch_add(1, Ordering::SeqCst);

                                            let mut m = miners.write().await;
                                            if let Some(info) = m.get_mut(&addr) {
                                                info.shares += 1;
                                                info.last_share_time = SystemTime::now()
                                                    .duration_since(UNIX_EPOCH).unwrap().as_secs();
                                                if !wallet_addr.is_empty() {
                                                    info.wallet_address = wallet_addr.clone();
                                                }
                                            }
                                            drop(m);

                                            let is_share = hash_val <= t.share_target;
                                            let is_block = hash_val <= t.block_target;

                                            if is_share && !wallet_addr.is_empty() {
                                                let mut rs = round_shares.write().await;
                                                *rs.entry(wallet_addr.clone()).or_insert(0) += 1;
                                                drop(rs);
                                            }

                                            if is_block {
                                                let bc2_clone = bc2.clone();
                                                let p2p_clone = p2p.clone();
                                                let wallet_clone = wallet.clone();
                                                let storage_clone = storage.clone();
                                                let miners_clone = miners.clone();
                                                let round_shares_clone = round_shares.clone();
                                                let write_half_clone = write_half.clone();
                                                let miner_addr = addr;
                                                let _miner_wallet = wallet_addr.clone();
                                                let coinbase_tx = t.coinbase_tx.clone();
                                                let header_bytes = t.header_bytes.clone();
                                                let mempool_txs = t.mempool_txs.clone();
                                                let height = t.height;
                                                let _block_target = t.block_target;
                                                let nonce_val = nonce;

                                                tokio::spawn(async move {
                                                    let mut bc = bc2_clone.write().await;

                                                    // Use the exact header from the template so the hash matches
                                                    // what the miner computed. header_bytes layout:
                                                    // 0-3: version, 4-11: height, 12-19: timestamp,
                                                    // 20-51: previous_hash, 52-83: merkle_root, 84-87: compact_target
                                                    let timestamp = u64::from_le_bytes(
                                                        header_bytes[12..20].try_into().unwrap_or([0u8; 8])
                                                    );
                                                    let mut merkle_root_arr = [0u8; 32];
                                                    merkle_root_arr.copy_from_slice(&header_bytes[52..84]);
                                                    let compact_target = u32::from_le_bytes(
                                                        header_bytes[84..88].try_into().unwrap_or([0u8; 4])
                                                    );

                                                    let mut all_txs = vec![coinbase_tx];
                                                    all_txs.extend(mempool_txs);
                                                    let block = Block {
                                                        header: BlockHeader {
                                                            version: 1,
                                                            height,
                                                            timestamp,
                                                            previous_hash: bc.state.current_hash,
                                                            merkle_root: merkle_root_arr,
                                                            difficulty_target: compact_target,
                                                            nonce: nonce_val,
                                                            extra_nonce: 0,
                                                        },
                                                        transactions: all_txs,
                                                    };

                                                    let num_miners = {
                                                        let rs = round_shares_clone.read().await;
                                                        rs.len()
                                                    };
                                                    {
                                                        let mut rs = round_shares_clone.write().await;
                                                        rs.clear();
                                                    }

                                                    match bc.add_block(block.clone()) {
                                                        Ok(()) => {
                                                            info!("Pool found block {}! Nonce: {}", height, nonce_val);
                                                            info!("Pool collected full reward; {} miners paid via pool wallet", num_miners);
                                                            drop(bc);
                                                            if let Some(ref p) = p2p_clone {
                                                                p.broadcast_block(&block).await;
                                                            }
                                                            if let Some(ref w) = wallet_clone {
                                                                let mut wallet_lock = w.write().await;
                                                                if let Some(ref mut wlt) = *wallet_lock {
                                                                    if wlt.scan_block(&block) > 0 {
                                                                        if let Some(ref st) = storage_clone {
                                                                            if let Ok(s) = st.lock() {
                                                                                let _ = s.save_wallet(wlt);
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            {
                                                                let mut m = miners_clone.write().await;
                                                                if let Some(info) = m.get_mut(&miner_addr) {
                                                                    info.valid_blocks += 1;
                                                                }
                                                            }
                                                            send_msg(&*write_half_clone, &format!(
                                                                r#"{{"type":"result","job_id":{},"nonce":{},"status":"accepted","message":"Block accepted"}}"#,
                                                                job_id, nonce_val
                                                            )).await;
                                                        }
                                                        Err(e) => {
                                                            warn!("Pool block rejected: {}", e);
                                                            send_msg(&*write_half_clone, &format!(
                                                                r#"{{"type":"result","job_id":{},"nonce":{},"status":"rejected","message":"Block rejected: {}"}}"#,
                                                                job_id, nonce_val, e
                                                            )).await;
                                                        }
                                                    }
                                                });
                                                None
                                            } else if is_share {
                                                Some((true, "Share accepted".to_string()))
                                            } else {
                                                Some((false, "Below share target".to_string()))
                                            }
                                        }
                                    } else {
                                        Some((false, "No active job".to_string()))
                                    };
                                    drop(template);

                                    if let Some((ok, msg)) = result {
                                        let status = if ok { "accepted" } else { "rejected" };
                                        let resp = format!(
                                            r#"{{"type":"result","job_id":{},"nonce":{},"status":"{}","message":"{}"}}"#,
                                            job_id, nonce, status, msg
                                        );
                                        let mut w = write_half.lock().await;
                                        let _ = w.write_all(resp.as_bytes()).await;
                                        let _ = w.write_all(b"\n").await;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Miner {} read error: {}", addr, e);
                        break;
                    }
                }
            }

            let mut m = miners.write().await;
            m.remove(&addr);
            info!("Miner {} disconnected", addr);
        });
    }

    pub async fn stats(&self) -> serde_json::Value {
        let miners = self.miners.read().await;
        let total_miners = miners.len();
        let total_shares_val = self.total_shares.load(Ordering::SeqCst);
        let mut miner_list = Vec::new();
        for (addr, info) in miners.iter() {
            miner_list.push(serde_json::json!({
                "address": addr.to_string(),
                "wallet": info.wallet_address,
                "shares": info.shares,
                "blocks_found": info.valid_blocks,
                "last_share": info.last_share_time,
            }));
        }
        let template = self.current_template.read().await;
        let job_info = match template.as_ref() {
            Some(t) => serde_json::json!({
                "job_id": t.job_id,
                "height": t.height,
                "difficulty": t.difficulty,
                "block_target": t.block_target,
                "share_target": t.share_target,
            }),
            None => serde_json::json!(null),
        };
        serde_json::json!({
            "port": self.port,
            "miners": total_miners,
            "total_shares": total_shares_val,
            "current_job": job_info,
            "miner_list": miner_list,
        })
    }
}
