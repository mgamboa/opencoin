use std::net::SocketAddr;
use tokio::sync::mpsc;
use log::{info, warn, error};

use crate::chain::block::BlockHeader;
use crate::crypto::hash::verify_merkle_proof;
use crate::crypto::bloom::BloomFilter;
use crate::p2p::{Message, encode_length_prefixed, read_message_inner};

pub struct LightClient {
    pub headers: Vec<BlockHeader>,
    pub best_height: u64,
    pub connected_peer: Option<SocketAddr>,
    writer: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl LightClient {
    pub fn new() -> Self {
        LightClient {
            headers: Vec::new(),
            best_height: 0,
            connected_peer: None,
            writer: None,
        }
    }

    pub async fn connect(&mut self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::AsyncWriteExt;

        let stream = tokio::net::TcpStream::connect(addr).await?;
        let (mut read_half, mut write_half) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let reader_tx = tx.clone();

        self.connected_peer = Some(addr);
        self.writer = Some(tx.clone());

        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if let Err(e) = write_half.write_all(&data).await {
                    error!("Light client write error: {}", e);
                    break;
                }
            }
        });

        tokio::spawn(async move {
            loop {
                match read_message_inner(&mut read_half).await {
                    Ok(Some(msg)) => {
                        match msg {
                            Message::Headers(new_headers) => {
                                info!("Received {} headers", new_headers.len());
                            }
                            Message::MerkleBlock { block_height, merkle_root, tx_hashes, matching_indices, merkle_proofs } => {
                                info!("MerkleBlock at height {} with {} matching txs", block_height, matching_indices.len());
                                for (&i, proof) in matching_indices.iter().zip(merkle_proofs.iter()) {
                                    if i < tx_hashes.len() {
                                        let valid = verify_merkle_proof(&tx_hashes[i], proof, i, &merkle_root);
                                        if valid {
                                            info!("SPV: Verified tx[{}] inclusion in block {}", i, block_height);
                                        } else {
                                            warn!("SPV: Invalid proof for tx[{}] in block {}", i, block_height);
                                        }
                                    }
                                }
                            }
                            Message::Ping(version) => {
                                let _ = reader_tx.send(encode_length_prefixed(&bincode::serialize(&Message::Pong(version)).unwrap()));
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => {
                        info!("Disconnected from peer");
                        break;
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn send(&self, msg: &Message) {
        if let Some(ref writer) = self.writer {
            if let Ok(data) = bincode::serialize(msg) {
                let _ = writer.send(encode_length_prefixed(&data));
            }
        }
    }

    pub async fn sync_headers(&self, from_height: u64) {
        self.send(&Message::GetHeaders(from_height)).await;
    }

    pub async fn request_merkle_block(&self, height: u64, filter: BloomFilter) {
        self.send(&Message::GetMerkleBlock { height, filter }).await;
    }

    pub fn is_connected(&self) -> bool {
        self.connected_peer.is_some()
    }
}
