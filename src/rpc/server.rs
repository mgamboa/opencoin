use std::sync::Arc;
use tokio::sync::RwLock;

use crate::chain::block::Block;
use crate::chain::transaction::Transaction;
use crate::chain::blockchain::Blockchain;
use crate::wallet::Wallet;
use crate::p2p::P2PNetwork;

pub struct RpcServer {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Option<Wallet>>>,
    pub p2p: Arc<P2PNetwork>,
    pub port: u16,
}

impl RpcServer {
    pub fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Option<Wallet>>>,
        p2p: Arc<P2PNetwork>,
        port: u16,
    ) -> Self {
        RpcServer {
            blockchain,
            wallet,
            p2p,
            port,
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        log::info!("RPC server listening on {}", addr);

        let blockchain = self.blockchain.clone();
        let wallet = self.wallet.clone();
        let p2p = self.p2p.clone();

        loop {
            let (mut stream, peer) = listener.accept().await?;
            let blockchain = blockchain.clone();
            let wallet = wallet.clone();
            let p2p = p2p.clone();

            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split};

                let (read_half, mut write_half) = split(stream);
                let mut reader = BufReader::new(read_half);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let response = match serde_json::from_str::<serde_json::Value>(&line) {
                                Ok(req) => {
                                    let method = req["method"].as_str().unwrap_or("");
                                    let params = req["params"].as_array().cloned().unwrap_or_default();
                                    let id = req["id"].clone();
                                    handle_rpc_request(method, &params, &id, &blockchain, &wallet, &p2p).await
                                }
                                Err(_) => {
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "error": {"code": -32700, "message": "Parse error"},
                                        "id": null
                                    })
                                }
                            };
                            let mut response_str = serde_json::to_string(&response).unwrap_or_default();
                            response_str.push('\n');
                            let _ = write_half.write_all(response_str.as_bytes()).await;
                        }
                        Err(e) => {
                            log::error!("RPC read error: {}", e);
                            break;
                        }
                    }
                }
            });
        }
    }
}

async fn handle_rpc_request(
    method: &str,
    params: &[serde_json::Value],
    id: &serde_json::Value,
    blockchain: &Arc<RwLock<Blockchain>>,
    wallet: &Arc<RwLock<Option<Wallet>>>,
    p2p: &Arc<P2PNetwork>,
) -> serde_json::Value {
    let result = match method {
        "getinfo" => {
            let bc = blockchain.read().await;
            serde_json::json!({
                "height": bc.state.height,
                "circulating_supply": bc.state.circulating_supply,
                "total_work": bc.state.total_work.to_string(),
                "version": crate::config::VERSION,
                "protocol": crate::PROTOCOL_VERSION,
            })
        }
        "getblock" => {
            let height = params.get(0).and_then(|p| p.as_u64()).unwrap_or(0);
            let bc = blockchain.read().await;
            if let Some(block) = bc.get_block(height) {
                serde_json::json!({
                    "height": block.header.height,
                    "hash": hex::encode(block.hash()),
                    "timestamp": block.header.timestamp,
                    "tx_count": block.transactions.len(),
                })
            } else {
                serde_json::json!({"error": "Block not found"})
            }
        }
        "getbalance" => {
            let w = wallet.read().await;
            if let Some(ref wallet) = *w {
                serde_json::json!({
                    "balance": wallet.balance,
                    "locked": wallet.locked_balance,
                    "address": wallet.address_string(),
                })
            } else {
                serde_json::json!({"error": "No wallet loaded"})
            }
        }
        "getaddress" => {
            let w = wallet.read().await;
            if let Some(ref wallet) = *w {
                serde_json::json!({"address": wallet.address_string()})
            } else {
                serde_json::json!({"error": "No wallet loaded"})
            }
        }
        _ => {
            serde_json::json!({
                "error": format!("Method '{}' not found", method)
            })
        }
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "result": result,
        "id": id
    })
}
