use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use log::{info, warn, error};

use crate::config;
use crate::chain::block::Block;
use crate::chain::transaction::Transaction;

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

pub struct Peer {
    pub address: SocketAddr,
    pub connection: TcpStream,
    pub height: u64,
    pub version: u32,
}

pub struct P2PNetwork {
    pub peers: Arc<RwLock<Vec<Peer>>>,
    pub mempool: Arc<RwLock<Vec<Transaction>>>,
    pub our_address: SocketAddr,
    pub known_peers: Arc<RwLock<Vec<SocketAddr>>>,
}

impl P2PNetwork {
    pub fn new(port: u16) -> Self {
        P2PNetwork {
            peers: Arc::new(RwLock::new(Vec::new())),
            mempool: Arc::new(RwLock::new(Vec::new())),
            our_address: format!("0.0.0.0:{}", port).parse().unwrap(),
            known_peers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.our_address).await?;
        info!("P2P server listening on {}", self.our_address);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New peer connected: {}", addr);
                    tokio::spawn(handle_peer(stream, addr));
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    pub async fn connect_to_peer(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to peer: {}", addr);
        let mut peer = Peer {
            address: addr,
            connection: stream,
            height: 0,
            version: 1,
        };

            let ping_msg = Message::Ping(crate::PROTOCOL_VERSION as u64);
        let data = bincode::serialize(&ping_msg)?;
        // Need to send data - simplified for now
        peer.connection.writable().await?;
        // peer.connection.try_write(&data)?;

        let mut peers = self.peers.write().await;
        peers.push(peer);
        Ok(())
    }

    pub async fn broadcast_block(&self, block: &Block) -> Result<(), Box<dyn std::error::Error>> {
        let msg = Message::Block(Box::new(block.clone()));
        let data = bincode::serialize(&msg)?;
        let peers = self.peers.read().await;
        for peer in peers.iter() {
            // peer.connection.writable().await?;
            // peer.connection.try_write(&data)?;
        }
        Ok(())
    }

    pub async fn broadcast_transaction(&self, tx: &Transaction) -> Result<(), Box<dyn std::error::Error>> {
        let msg = Message::Transaction(Box::new(tx.clone()));
        let data = bincode::serialize(&msg)?;
        let peers = self.peers.read().await;
        for peer in peers.iter() {
            // Simplified
        }
        Ok(())
    }

    pub async fn add_to_mempool(&self, tx: Transaction) {
        let mut mempool = self.mempool.write().await;
        if !mempool.iter().any(|t| t.hash() == tx.hash()) {
            mempool.push(tx);
        }
    }
}

async fn handle_peer(mut stream: TcpStream, addr: SocketAddr) {
    info!("Handling peer: {}", addr);
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            read_message(&mut stream),
        )
        .await
        {
            Ok(Ok(msg)) => {
                match msg {
                    Message::Ping(_version) => {
                        let pong = Message::Pong(crate::PROTOCOL_VERSION as u64);
                        if let Ok(data) = bincode::serialize(&pong) {
                            let _ = stream.try_write(&data);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Err(_)) => {
                warn!("Peer {} disconnected", addr);
                break;
            }
            Err(_) => {
                info!("Peer {} timed out", addr);
                break;
            }
        }
    }
}

async fn read_message(stream: &mut TcpStream) -> Result<Message, Box<dyn std::error::Error>> {
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    let msg: Message = bincode::deserialize(&data)?;
    Ok(msg)
}
