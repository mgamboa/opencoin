use std::sync::Arc;
use tokio::sync::RwLock;

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
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        log::info!("RPC server listening on {}", addr);

        let blockchain = self.blockchain.clone();
        let wallet = self.wallet.clone();
        let p2p = self.p2p.clone();

        loop {
            let (stream, _peer) = listener.accept().await?;
            let blockchain = blockchain.clone();
            let wallet = wallet.clone();
            let p2p = p2p.clone();

            tokio::spawn(async move {
                handle_connection(stream, blockchain, wallet, p2p).await;
            });
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Option<Wallet>>>,
    p2p: Arc<P2PNetwork>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();

    if reader.read_line(&mut request_line).await.is_err() {
        return;
    }

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    let method = parts.get(0).copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("");

    let mut content_length = 0usize;
    let mut body = String::new();

    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await.ok() != Some(0) && header != "\r\n" && header != "\n" {
            let header = header.trim().to_lowercase();
            if header.starts_with("content-length:") {
                if let Ok(len) = header.trim_start_matches("content-length:").trim().parse::<usize>() {
                    content_length = len;
                }
            }
            if header.is_empty() {
                break;
            }
        } else {
            break;
        }
    }

    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        if reader.read_exact(&mut buf).await.is_ok() {
            body = String::from_utf8_lossy(&buf).to_string();
        }
    }

    let response = match (method, path) {
        ("GET", "/") => {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
                r##"<!DOCTYPE html>
<html><head><title>OpenCoin Node</title>
<style>body{font-family:monospace;margin:40px;background:#111;color:#0f0}
pre{font-size:14px}</style></head>
<body>
<h1>OpenCoin Node</h1>
<pre id=info>Loading...</pre>
<script>
async function getInfo(){try{
let r=await fetch('/rpc',{method:'POST',headers:{'Content-Type':'application/json'},
body:JSON.stringify({jsonrpc:'2.0',method:'getinfo',params:[],id:1})});
let d=await r.json();
document.getElementById('info').textContent=JSON.stringify(d.result,null,2);
}catch(e){document.getElementById('info').textContent='Error: '+e}}
setInterval(getInfo,2000);getInfo();
</script></body></html>"##
            )
        }
        ("POST", "/") | ("POST", "/rpc") | ("POST", "/api") => {
            let response = match serde_json::from_str::<serde_json::Value>(&body) {
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
            let body = serde_json::to_string(&response).unwrap_or_default();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
        }
        ("OPTIONS", _) => {
            "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        }
        _ => {
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"not found\"}".to_string()
        }
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

async fn handle_rpc_request(
    method: &str,
    params: &[serde_json::Value],
    id: &serde_json::Value,
    blockchain: &Arc<RwLock<Blockchain>>,
    wallet: &Arc<RwLock<Option<Wallet>>>,
    _p2p: &Arc<P2PNetwork>,
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
                    "address": wallet.address_string().unwrap_or_default(),
                })
            } else {
                serde_json::json!({"error": "No wallet loaded"})
            }
        }
        "getaddress" => {
            let w = wallet.read().await;
            if let Some(ref wallet) = *w {
                serde_json::json!({"address": wallet.address_string().unwrap_or_default()})
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
