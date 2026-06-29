use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use log::{info, warn, error};

use crate::chain::block::{Block, BlockHeader, calculate_block_reward};
use crate::chain::transaction::Transaction;
use crate::chain::blockchain::Blockchain;
use crate::crypto::hash::merkle_root;
use crate::crypto::stealth::StealthAddress;
use crate::consensus::difficulty::{calculate_difficulty, difficulty_to_compact};
use crate::consensus::pow::{calculate_target, check_pow};

#[derive(Debug, Clone)]
pub struct MinerInfo {
    pub address: SocketAddr,
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
}

pub struct PoolServer {
    pub port: u16,
    pub pool_address: StealthAddress,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub miners: Arc<RwLock<HashMap<SocketAddr, MinerInfo>>>,
    pub current_template: Arc<RwLock<Option<BlockTemplate>>>,
    pub job_counter: Arc<AtomicU64>,
    pub total_shares: Arc<AtomicU64>,
    pub running: Arc<AtomicBool>,
}

impl PoolServer {
    pub fn new(port: u16, pool_address: StealthAddress, blockchain: Arc<RwLock<Blockchain>>) -> Self {
        PoolServer {
            port,
            pool_address,
            blockchain,
            miners: Arc::new(RwLock::new(HashMap::new())),
            current_template: Arc::new(RwLock::new(None)),
            job_counter: Arc::new(AtomicU64::new(1)),
            total_shares: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("Pool server listening on {}", addr);

        let template_updater = self.current_template.clone();
        let blockchain = self.blockchain.clone();
        let pool_addr = self.pool_address.clone();
        let job_counter = self.job_counter.clone();

        let updater = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let bc = blockchain.read().await;
                let height = bc.state.height + 1;
                let reward = calculate_block_reward(height, bc.state.premine_remaining);
                let coinbase_tx = Transaction::coinbase(reward, &pool_addr);
                let txs = vec![coinbase_tx.clone()];
                let tx_hashes: Vec<[u8; 32]> = txs.iter().map(|t| t.hash()).collect();
                let merkle = merkle_root(&tx_hashes);

                let difficulty = calculate_difficulty(&bc.blocks);
                let compact_target = difficulty_to_compact(difficulty);
                let block_target = calculate_target(difficulty);
                let share_target = if block_target < u64::MAX / 100 {
                    block_target * 100
                } else {
                    u64::MAX / 1000
                };

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

                let template = BlockTemplate {
                    height,
                    difficulty,
                    block_target,
                    share_target,
                    job_id,
                    header_bytes: header_data,
                    coinbase_tx,
                };

                let mut current = template_updater.write().await;
                let should_update = match current.as_ref() {
                    Some(old) => old.height != height || old.job_id != job_id,
                    None => true,
                };
                if should_update {
                    info!("New pool job #{} at height {}", job_id, height);
                    *current = Some(template);
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

    async fn handle_miner(&self, mut stream: TcpStream, addr: SocketAddr) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let miners = self.miners.clone();
        let current_template = self.current_template.clone();
        let blockchain = self.blockchain.clone();
        let total_shares = self.total_shares.clone();
        let pool_address = self.pool_address.clone();

        {
            let mut m = miners.write().await;
            m.insert(addr, MinerInfo {
                address: addr,
                shares: 0,
                last_share_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                valid_blocks: 0,
            });
        }

        let (read_half, mut write_half) = tokio::io::split(stream);

        async fn send_msg<W: tokio::io::AsyncWriteExt + Unpin>(w: &mut W, msg: &str) {
            let mut line = msg.to_string();
            line.push('\n');
            let _ = w.write_all(line.as_bytes()).await;
        }

        let template = current_template.read().await;
        if let Some(t) = template.as_ref() {
            let job_json = format!(
                r#"{{"type":"job","job_id":{},"height":{},"target":{},"share_target":{},"header":"{}"}}"#,
                t.job_id, t.height, t.block_target, t.share_target, hex::encode(&t.header_bytes)
            );
            send_msg(&mut write_half, &job_json).await;
        }
        drop(template);

        let miner_info = self.miners.clone();
        let ct = self.current_template.clone();
        let bc2 = self.blockchain.clone();

        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
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
                                            }
                                            drop(m);

                                            let is_share = hash_val <= t.share_target;
                                            let is_block = hash_val <= t.block_target;

                                            if is_block {
                                                let mut bc = bc2.write().await;
                                                let reward = calculate_block_reward(t.height, bc.state.premine_remaining);
                                                let coinbase = Transaction::coinbase(reward, &pool_address);
                                                let tx_hashes = vec![coinbase.hash()];
                                                let merkle = merkle_root(&tx_hashes);

                                                let block = Block {
                                                    header: BlockHeader {
                                                        version: 1,
                                                        height: t.height,
                                                        timestamp: SystemTime::now()
                                                            .duration_since(UNIX_EPOCH).unwrap().as_secs(),
                                                        previous_hash: bc.state.current_hash,
                                                        merkle_root: merkle,
                                                        difficulty_target: difficulty_to_compact(t.difficulty),
                                                        nonce,
                                                        extra_nonce: 0,
                                                    },
                                                    transactions: vec![coinbase],
                                                };

                                                match bc.add_block(block.clone()) {
                                                    Ok(()) => {
                                                        info!("Pool found block {}! Nonce: {}", t.height, nonce);
                                                        let mut m = miners.write().await;
                                                        if let Some(info) = m.get_mut(&addr) {
                                                            info.valid_blocks += 1;
                                                        }
                                                        drop(m);
                                                        Some((true, "Block accepted".to_string()))
                                                    }
                                                    Err(e) => {
                                                        warn!("Pool block rejected: {}", e);
                                                        Some((false, format!("Block rejected: {}", e)))
                                                    }
                                                }
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
                                        let _ = write_half.write_all(resp.as_bytes()).await;
                                        let _ = write_half.write_all(b"\n").await;
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
