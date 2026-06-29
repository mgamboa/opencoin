use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};
use serde::{Deserialize, Serialize};
use log::{info, warn, error};

use crate::chain::block::Block;
use crate::chain::transaction::Transaction;
use crate::chain::blockchain::Blockchain;

type PeerMap = HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>;

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
}

pub struct P2PNetwork {
    pub peers: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    pub mempool: Arc<RwLock<Vec<Transaction>>>,
    pub our_address: SocketAddr,
    pub known_peers: Arc<RwLock<Vec<SocketAddr>>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
}

impl P2PNetwork {
    pub fn new(port: u16, blockchain: Arc<RwLock<Blockchain>>) -> Self {
        P2PNetwork {
            peers: Arc::new(RwLock::new(HashMap::new())),
            mempool: Arc::new(RwLock::new(Vec::new())),
            our_address: format!("0.0.0.0:{}", port).parse().unwrap(),
            known_peers: Arc::new(RwLock::new(Vec::new())),
            blockchain,
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.our_address).await?;
        info!("P2P server listening on {}", self.our_address);

        let known_peers = self.known_peers.clone();
        let peers = self.peers.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                let known = known_peers.read().await;
                let peer_addrs: Vec<SocketAddr> = known.iter().cloned().collect();
                drop(known);
                for peer_addr in peer_addrs {
                    let p = peers.read().await;
                    if let Some(tx) = p.get(&peer_addr) {
                        let msg = bincode::serialize(&Message::GetPeers).unwrap_or_default();
                        let data = encode_length_prefixed(&msg);
                        let _ = tx.send(data);
                    }
                }
                info!("Peer discovery: requested peers from connected nodes");
            }
        });

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New peer connected: {}", addr);
                    {
                        let mut known = self.known_peers.write().await;
                        if !known.contains(&addr) {
                            known.push(addr);
                        }
                    }
                    self.spawn_peer(stream, addr).await;
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
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
        self.send_to(&addr, &Message::Ping(crate::PROTOCOL_VERSION as u64)).await;

        {
            let mut known = self.known_peers.write().await;
            if !known.contains(&addr) {
                known.push(addr);
            }
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

        let peers_writer = self.peers.clone();

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
            info!("Peer {} write task ended", addr);
        });

        let peers_clone = self.peers.clone();
        let blockchain = self.blockchain.clone();
        let known_peers = self.known_peers.clone();
        let mempool = self.mempool.clone();
        let our_addr = self.our_address;

        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut read_half = read_half;
            loop {
                let result = read_message_inner(&mut read_half).await;
                match result {
                    Ok(Some(msg)) => {
                        let peers_snapshot = peers_clone.read().await.clone();
                        handle_message2(msg, &addr, peers_snapshot, &blockchain, &known_peers, &mempool, our_addr).await;
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

fn encode_length_prefixed(data: &[u8]) -> Vec<u8> {
    let len = (data.len() as u32).to_le_bytes();
    [&len, data].concat()
}

async fn read_message_inner<R>(read_half: &mut R) -> Result<Option<Message>, Box<dyn std::error::Error + Send + Sync>>
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
            if height > bc.state.height {
                match bc.add_block(*block) {
                    Ok(()) => info!("Added block {} from peer", height),
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
            let mut bc = blockchain.write().await;
            for block in blocks {
                let height = block.header.height;
                if height > bc.state.height {
                    if let Err(e) = bc.add_block(block) {
                        warn!("Failed to add block {} from peer: {}", height, e);
                        break;
                    }
                    info!("Synced block {}", height);
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
            for paddr in peer_list {
                if paddr == our_address { continue; }
                let mut known = known_peers.write().await;
                if !known.contains(&paddr) {
                    known.push(paddr);
                }
            }
            info!("Discovered {} peers from {}", count, addr);
        }
        Message::MempoolRequest => {}
        Message::MempoolResponse(_) => {}
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
