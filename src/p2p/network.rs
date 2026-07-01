use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};
use serde::{Deserialize, Serialize};
use log::{info, warn, error};

use crate::chain::block::{Block, BlockHeader};
use crate::chain::transaction::Transaction;
use crate::chain::blockchain::Blockchain;
use crate::config;
use crate::crypto::bloom::BloomFilter;
use crate::wallet::Wallet;
use crate::storage::db::Storage;

const PEER_TIMEOUT_SECS: u64 = 180;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Ping(u64),
    Pong(u64),
    Block(Box<Block>),
    Transaction(Box<Transaction>),
    GetBlocks(u64),
    Blocks(Vec<Block>),
    GetPeers,
    Peers(Vec<SocketAddr>),
    MempoolRequest,
    MempoolResponse(Vec<Transaction>),
    GetHeaders(u64),
    Headers(Vec<BlockHeader>),
    GetMerkleBlock { height: u64, filter: BloomFilter },
    MerkleBlock {
        block_height: u64,
        merkle_root: [u8; 32],
        tx_hashes: Vec<[u8; 32]>,
        matching_indices: Vec<usize>,
        merkle_proofs: Vec<Vec<[u8; 32]>>,
    },
}

pub struct P2PNetwork {
    pub peers: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    pub mempool: Arc<RwLock<Vec<Transaction>>>,
    pub our_address: SocketAddr,
    pub public_ip: Option<SocketAddr>,
    pub known_peers: Arc<RwLock<Vec<SocketAddr>>>,
    pub peer_last_seen: Arc<RwLock<HashMap<SocketAddr, tokio::time::Instant>>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Option<Arc<RwLock<Option<Wallet>>>>,
    pub storage: Option<Arc<std::sync::Mutex<Storage>>>,
}

impl P2PNetwork {
    pub fn new(port: u16, blockchain: Arc<RwLock<Blockchain>>) -> Self {
        P2PNetwork {
            peers: Arc::new(RwLock::new(HashMap::new())),
            mempool: Arc::new(RwLock::new(Vec::new())),
            our_address: format!("0.0.0.0:{}", port).parse().unwrap(),
            public_ip: None,
            known_peers: Arc::new(RwLock::new(Vec::new())),
            peer_last_seen: Arc::new(RwLock::new(HashMap::new())),
            blockchain,
            wallet: None,
            storage: None,
        }
    }

    pub fn with_public_ip(mut self, ip: SocketAddr) -> Self {
        self.public_ip = Some(ip);
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

    pub fn load_peers_from_storage_sync(&self) {
        if let Some(ref st) = self.storage {
            if let Ok(s) = st.lock() {
                if let Ok(peers) = s.load_peers() {
                    if let Ok(mut known) = self.known_peers.try_write() {
                        *known = peers;
                        log::info!("Loaded {} peers from storage", known.len());
                    }
                }
            }
        }
    }

    fn save_peers_sync(&self) {
        if let Some(ref st) = self.storage {
            if let Ok(known) = self.known_peers.try_read() {
                if let Ok(s) = st.lock() {
                    let _ = s.save_peers(&known);
                }
            }
        }
    }

    fn save_peers_json(&self) {
        if let Ok(known) = self.known_peers.try_read() {
            let addrs: Vec<String> = known.iter().map(|a| a.to_string()).collect();
            if let Ok(json) = serde_json::to_string_pretty(&addrs) {
                let _ = std::fs::write("peers.json", json);
            }
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.our_address).await?;
        info!("P2P server listening on {}", self.our_address);

        let known_peers = self.known_peers.clone();
        let peers = self.peers.clone();
        let peer_last_seen = self.peer_last_seen.clone();
        let public_ip = self.public_ip;

        // Channel for reconnection requests from the maintenance task
        let (reconnect_tx, mut reconnect_rx) = tokio::sync::mpsc::unbounded_channel::<SocketAddr>();

        // Periodic maintenance task: peer discovery, health check, reconnect
        let maint_known = known_peers.clone();
        let maint_peers = peers.clone();
        let maint_last_seen = peer_last_seen.clone();
        let maint_reconnect = reconnect_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

                // Announce our public IP to all connected peers
                if let Some(our_ip) = public_ip {
                    let my_peers_msg = Message::Peers(vec![our_ip]);
                    let p = maint_peers.read().await;
                    for (_, tx) in p.iter() {
                        let msg = bincode::serialize(&my_peers_msg).unwrap_or_default();
                        let data = encode_length_prefixed(&msg);
                        let _ = tx.send(data);
                    }
                    drop(p);
                }

                // Request peers from all connected
                let known = maint_known.read().await;
                let peer_addrs: Vec<SocketAddr> = known.iter().cloned().collect();
                drop(known);
                for peer_addr in &peer_addrs {
                    let p = maint_peers.read().await;
                    if let Some(tx) = p.get(peer_addr) {
                        let msg = bincode::serialize(&Message::GetPeers).unwrap_or_default();
                        let data = encode_length_prefixed(&msg);
                        let _ = tx.send(data);
                    }
                    drop(p);
                }
                info!("Peer discovery: announced self + requested peers");

                // Health check: disconnect stale peers
                let now = tokio::time::Instant::now();
                let mut stale_addrs = Vec::new();
                {
                    let last_seen = maint_last_seen.read().await;
                    let p = maint_peers.read().await;
                    for (addr, _) in p.iter() {
                        let last = last_seen.get(addr).copied().unwrap_or(now);
                        if now.duration_since(last).as_secs() > PEER_TIMEOUT_SECS {
                            stale_addrs.push(*addr);
                            warn!("Peer {} stale (last seen {}s ago), disconnecting", addr, now.duration_since(last).as_secs());
                        }
                    }
                }
                for addr in &stale_addrs {
                    let mut p = maint_peers.write().await;
                    p.remove(addr);
                    maint_last_seen.write().await.remove(addr);
                    info!("Removed stale peer {}", addr);
                }

                // Try to maintain minimum peers
                let connected_count = maint_peers.read().await.len();
                if connected_count < config::MINIMUM_PEERS {
                    let known = maint_known.read().await;
                    let connected = maint_peers.read().await;
                    for addr in known.iter() {
                        if !connected.contains_key(addr) {
                            let _ = maint_reconnect.send(*addr);
                            if connected_count >= config::MINIMUM_PEERS {
                                break;
                            }
                        }
                    }
                }
            }
        });

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            info!("New peer connected: {}", addr);
                            {
                                let mut known = self.known_peers.write().await;
                                if !known.contains(&addr) {
                                    known.push(addr);
                                    self.save_peers_sync();
                                    self.save_peers_json();
                                }
                            }
                            self.spawn_peer(stream, addr).await;
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                Some(addr) = reconnect_rx.recv() => {
                    if let Err(e) = self.connect_to_peer(addr).await {
                        warn!("Failed to reconnect to {}: {}", addr, e);
                    }
                }
            }
        }
    }

    pub async fn connect_to_peer(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.peers.read().await.contains_key(&addr) {
            return Ok(());
        }
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to peer: {}", addr);
        self.spawn_peer(stream, addr).await;

        // Announce our public IP if set
        if let Some(our_ip) = self.public_ip {
            let announce = Message::Peers(vec![our_ip]);
            self.send_to(&addr, &announce).await;
            info!("Announced our public IP {} to peer {}", our_ip, addr);
        }

        self.send_to(&addr, &Message::Ping(crate::PROTOCOL_VERSION as u64)).await;

        {
            let mut known = self.known_peers.write().await;
            if !known.contains(&addr) {
                known.push(addr);
                self.save_peers_sync();
                self.save_peers_json();
            }
        }

        {
            let mut last_seen = self.peer_last_seen.write().await;
            last_seen.insert(addr, tokio::time::Instant::now());
        }

        Ok(())
    }

    async fn spawn_peer(&self, stream: TcpStream, addr: SocketAddr) {
        let (read_half, mut write_half) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        {
            let mut peers = self.peers.write().await;
            peers.insert(addr, tx);
        }

        {
            let mut last_seen = self.peer_last_seen.write().await;
            last_seen.insert(addr, tokio::time::Instant::now());
        }

        let peers_writer = self.peers.clone();
        let peer_last_seen_writer = self.peer_last_seen.clone();

        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            loop {
                match rx.recv().await {
                    Some(data) => {
                        if write_half.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            let mut p = peers_writer.write().await;
            p.remove(&addr);
            peer_last_seen_writer.write().await.remove(&addr);
            info!("Peer {} write task ended", addr);
        });

        let peers_clone = self.peers.clone();
        let blockchain = self.blockchain.clone();
        let known_peers = self.known_peers.clone();
        let mempool = self.mempool.clone();
        let wallet = self.wallet.clone();
        let storage = self.storage.clone();
        let our_addr = self.our_address;
        let peer_last_seen = self.peer_last_seen.clone();

        tokio::spawn(async move {
            let mut read_half = read_half;
            loop {
                let result = read_message_inner(&mut read_half).await;
                match result {
                    Ok(Some(msg)) => {
                        // Update last seen timestamp
                        peer_last_seen.write().await.insert(addr, tokio::time::Instant::now());
                        let peers_snapshot = peers_clone.read().await.clone();
                        handle_message2(msg, &addr, peers_snapshot, &blockchain, &known_peers, &mempool, &wallet, &storage, our_addr).await;
                    }
                    Ok(None) => {
                        info!("Peer {} disconnected", addr);
                        break;
                    }
                    Err(e) => {
                        warn!("Peer {} error: {}", addr, e);
                        break;
                    }
                }
            }
            let mut p = peers_clone.write().await;
            p.remove(&addr);
            peer_last_seen.write().await.remove(&addr);
            info!("Peer {} read task ended", addr);
        });
    }

    pub async fn broadcast_message(&self, msg: &Message) {
        let data = match bincode::serialize(msg) {
            Ok(d) => d,
            Err(_) => return,
        };
        let data = encode_length_prefixed(&data);
        let peers = self.peers.read().await;
        for (addr, tx) in peers.iter() {
            if let Err(e) = tx.send(data.clone()) {
                warn!("Failed to send to {}: {}", addr, e);
            }
        }
    }

    pub async fn send_to(&self, addr: &SocketAddr, msg: &Message) {
        let data = match bincode::serialize(msg) {
            Ok(d) => encode_length_prefixed(&d),
            Err(_) => return,
        };
        let peers = self.peers.read().await;
        if let Some(tx) = peers.get(addr) {
            let _ = tx.send(data);
        }
    }

    pub async fn broadcast_block(&self, block: &Block) {
        self.broadcast_message(&Message::Block(Box::new(block.clone()))).await;
    }

    pub async fn broadcast_transaction(&self, tx: &Transaction) {
        self.broadcast_message(&Message::Transaction(Box::new(tx.clone()))).await;
    }

    pub async fn add_to_mempool(&self, tx: Transaction) {
        let mut mempool = self.mempool.write().await;
        if !mempool.iter().any(|t| t.hash() == tx.hash()) {
            mempool.push(tx);
        }
    }
}

pub fn encode_length_prefixed(data: &[u8]) -> Vec<u8> {
    let len = (data.len() as u32).to_le_bytes();
    [&len, data].concat()
}

pub async fn read_message_inner<R>(read_half: &mut R) -> Result<Option<Message>, Box<dyn std::error::Error + Send + Sync>>
where
    R: tokio::io::AsyncReadExt + Unpin + Send,
{
    let mut len_buf = [0u8; 4];
    match read_half.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(Box::new(e)),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    read_half.read_exact(&mut data).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let msg: Message = bincode::deserialize(&data).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    Ok(Some(msg))
}

async fn handle_message2(
    msg: Message,
    addr: &SocketAddr,
    peers: HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>,
    blockchain: &Arc<RwLock<Blockchain>>,
    known_peers: &Arc<RwLock<Vec<SocketAddr>>>,
    mempool: &Arc<RwLock<Vec<Transaction>>>,
    wallet: &Option<Arc<RwLock<Option<Wallet>>>>,
    storage: &Option<Arc<std::sync::Mutex<Storage>>>,
    our_address: SocketAddr,
) {
    match msg {
        Message::Ping(version) => {
            info!("Ping from {} (v{})", addr, version);
            if let Some(tx) = peers.get(addr) {
                let pong = bincode::serialize(&Message::Pong(crate::PROTOCOL_VERSION as u64)).unwrap();
                let _ = tx.send(encode_length_prefixed(&pong));
            }
        }
        Message::Pong(version) => {
            info!("Pong from {} (v{})", addr, version);
        }
        Message::Block(block) => {
            info!("Received block {} from {}: hash={}", block.header.height, addr, hex::encode(block.hash()));
            let mut bc = blockchain.write().await;
            let height = block.header.height;
            let b = *block;
            if height > bc.state.height {
                match bc.add_block(b.clone()) {
                    Ok(()) => {
                        info!("Added block {} from peer", height);
                        drop(bc);
                        if let Some(ref w) = wallet {
                            let mut wallet_lock = w.write().await;
                            if let Some(ref mut wlt) = *wallet_lock {
                                if wlt.scan_block(&b) > 0 {
                                    if let Some(ref st) = storage {
                                        if let Ok(s) = st.lock() {
                                            let _ = s.save_wallet(wlt);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Failed to add block from peer {}: {}", addr, e),
                }
            }
        }
        Message::GetBlocks(from_height) => {
            let bc = blockchain.read().await;
            let mut blocks = Vec::new();
            for h in from_height..=bc.state.height {
                if let Some(block) = bc.get_block(h) {
                    blocks.push(block.clone());
                }
            }
            if !blocks.is_empty() {
                if let Some(tx) = peers.get(addr) {
                    let resp = bincode::serialize(&Message::Blocks(blocks)).unwrap();
                    let _ = tx.send(encode_length_prefixed(&resp));
                }
            }
        }
        Message::Blocks(blocks) => {
            info!("Received {} blocks from {}", blocks.len(), addr);
            if blocks.is_empty() { return; }
            let mut bc = blockchain.write().await;
            for block in blocks {
                let height = block.header.height;
                if height > bc.state.height || bc.get_block(height).is_none() {
                    match bc.add_block(block.clone()) {
                        Ok(()) => {
                            info!("Synced block {}", height);
                            drop(bc);
                            if let Some(ref w) = wallet {
                                let mut wl = w.write().await;
                                if let Some(ref mut wlt) = *wl {
                                    if wlt.scan_block(&block) > 0 {
                                        if let Some(ref st) = storage {
                                            if let Ok(s) = st.lock() {
                                                let _ = s.save_wallet(wlt);
                                            }
                                        }
                                    }
                                }
                            }
                            bc = blockchain.write().await;
                        }
                        Err(e) => {
                            warn!("Failed to add block {} from peer: {}", height, e);
                            break;
                        }
                    }
                }
            }
        }
        Message::GetPeers => {
            let known = known_peers.read().await;
            let peer_addrs: Vec<SocketAddr> = known
                .iter()
                .filter(|p| *p != addr && !peers.contains_key(p))
                .take(10)
                .cloned()
                .collect();
            drop(known);

            if let Some(tx) = peers.get(addr) {
                let resp = bincode::serialize(&Message::Peers(peer_addrs)).unwrap();
                let _ = tx.send(encode_length_prefixed(&resp));
            }
        }
        Message::Peers(peer_list) => {
            let count = peer_list.len();
            let mut changed = false;
            for paddr in peer_list {
                if paddr == our_address { continue; }
                let mut known = known_peers.write().await;
                if !known.contains(&paddr) {
                    known.push(paddr);
                    changed = true;
                }
                drop(known);
            }
            if changed {
                if let Some(ref st) = storage {
                    let known = known_peers.read().await;
                    if let Ok(s) = st.lock() {
                        let _ = s.save_peers(&known);
                    }
                }
                // Also persist to peers.json for new nodes
                let known = known_peers.read().await;
                let addrs: Vec<String> = known.iter().map(|a| a.to_string()).collect();
                if let Ok(json) = serde_json::to_string_pretty(&addrs) {
                    let _ = std::fs::write("peers.json", json);
                }
            }
            info!("Discovered {} peers from {}", count, addr);
        }
        Message::MempoolRequest => {
            if let Some(tx) = peers.get(addr) {
                let mp = mempool.read().await;
                let resp = bincode::serialize(&Message::MempoolResponse(mp.clone())).unwrap();
                let _ = tx.send(encode_length_prefixed(&resp));
            }
        }
        Message::MempoolResponse(txs) => {
            let mut mp = mempool.write().await;
            for tx in txs {
                if !mp.iter().any(|t| t.hash() == tx.hash()) {
                    mp.push(tx);
                }
            }
            info!("Received {} mempool transactions from {}", mp.len(), addr);
        }
        Message::GetHeaders(from_height) => {
            let bc = blockchain.read().await;
            let headers: Vec<BlockHeader> = bc.blocks.iter()
                .filter(|b| b.header.height >= from_height)
                .take(500)
                .map(|b| b.header.clone())
                .collect();
            if let Some(tx) = peers.get(addr) {
                let resp = bincode::serialize(&Message::Headers(headers)).unwrap();
                let _ = tx.send(encode_length_prefixed(&resp));
            }
        }
        Message::Headers(headers) => {
            let bc = blockchain.read().await;
            let new_count = headers.iter().filter(|h| {
                !bc.blocks.iter().any(|b| b.header.height == h.height)
            }).count();
            info!("Received {} headers ({} new) from {}", headers.len(), new_count, addr);
        }
        Message::GetMerkleBlock { height, filter } => {
            let bc = blockchain.read().await;
            if let Some(block) = bc.blocks.iter().find(|b| b.header.height == height) {
                let tx_hashes: Vec<[u8; 32]> = block.transactions.iter().map(|tx| tx.hash()).collect();
                let matching_indices: Vec<usize> = block.transactions.iter().enumerate()
                    .filter(|(_, tx)| {
                        filter.matches(&tx.hash()) || tx.outputs.iter().any(|o| {
                            filter.matches(&o.stealth_address.spend_pub.0) ||
                            filter.matches(&o.stealth_address.view_pub.0)
                        })
                    })
                    .map(|(i, _)| i)
                    .collect();
                let merkle_proofs: Vec<Vec<[u8; 32]>> = matching_indices.iter()
                    .map(|&i| crate::crypto::hash::merkle_proof(&tx_hashes, i))
                    .collect();
                if let Some(tx) = peers.get(addr) {
                    let resp = bincode::serialize(&Message::MerkleBlock {
                        block_height: height,
                        merkle_root: block.header.merkle_root,
                        tx_hashes,
                        matching_indices,
                        merkle_proofs,
                    }).unwrap();
                    let _ = tx.send(encode_length_prefixed(&resp));
                }
            }
        }
        Message::MerkleBlock { block_height, merkle_root, tx_hashes, matching_indices, merkle_proofs } => {
            info!("Received MerkleBlock for height {} from {} ({} matching txs)",
                block_height, addr, matching_indices.len());
            for (&i, proof) in matching_indices.iter().zip(merkle_proofs.iter()) {
                if i < tx_hashes.len() {
                    let valid = crate::crypto::hash::verify_merkle_proof(
                        &tx_hashes[i], proof, i, &merkle_root,
                    );
                    if valid {
                        info!("SPV: Merkle proof valid for tx[{}] in block {}", i, block_height);
                    } else {
                        warn!("SPV: Invalid merkle proof for tx[{}] in block {}", i, block_height);
                    }
                }
            }
        }
        Message::Transaction(tx) => {
            info!("Received transaction from {}: {}", addr, hex::encode(tx.hash()));
            let mut mp = mempool.write().await;
            if !mp.iter().any(|t| t.hash() == tx.hash()) {
                mp.push(*tx);
                info!("Transaction added to mempool");
            }
        }
    }
}
