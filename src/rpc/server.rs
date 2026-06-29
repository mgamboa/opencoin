use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::RwLock;

use crate::chain::blockchain::Blockchain;
use crate::wallet::Wallet;
use crate::p2p::P2PNetwork;
use crate::pool::PoolServer;
use crate::storage::db::Storage;

pub struct RpcServer {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Option<Wallet>>>,
    pub p2p: Arc<P2PNetwork>,
    pub pool: Option<Arc<PoolServer>>,
    pub port: u16,
    pub storage: Option<Arc<Mutex<Storage>>>,
}

impl RpcServer {
    pub fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Option<Wallet>>>,
        p2p: Arc<P2PNetwork>,
        pool: Option<Arc<PoolServer>>,
        port: u16,
    ) -> Self {
        RpcServer {
            blockchain,
            wallet,
            p2p,
            pool,
            port,
            storage: None,
        }
    }

    pub fn with_storage(mut self, storage: Arc<Mutex<Storage>>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        log::info!("RPC server listening on {}", addr);

        let blockchain = self.blockchain.clone();
        let wallet = self.wallet.clone();
        let p2p = self.p2p.clone();
        let pool = self.pool.clone();
        let storage = self.storage.clone();

        loop {
            let (stream, _peer) = listener.accept().await?;
            let blockchain = blockchain.clone();
            let wallet = wallet.clone();
            let p2p = p2p.clone();
            let pool = pool.clone();
            let storage = storage.clone();

            tokio::spawn(async move {
                handle_connection(stream, blockchain, wallet, p2p, pool, storage).await;
            });
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Option<Wallet>>>,
    p2p: Arc<P2PNetwork>,
    pool: Option<Arc<PoolServer>>,
    storage: Option<Arc<Mutex<Storage>>>,
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
        ("GET", "/") => html_page("OpenCoin Dashboard", &dashboard_html(&blockchain, &p2p).await),
        ("GET", "/wallet") => html_page("OpenCoin Wallet", &wallet_html(&wallet).await),
        ("GET", "/pool") => html_page("OpenCoin Pool", &pool_html(&pool).await),
        ("GET", "/blocks") => html_page("OpenCoin Blocks", &blocks_html(&blockchain).await),
        ("GET", "/download") => html_page("OpenCoin Download", DOWNLOAD_HTML),
        ("POST", "/") | ("POST", "/rpc") | ("POST", "/api") => {
                    let response = match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(req) => {
                    let method = req["method"].as_str().unwrap_or("");
                    let params = req["params"].as_array().cloned().unwrap_or_default();
                    let id = req["id"].clone();
                    handle_rpc_request(method, &params, &id, &blockchain, &wallet, &p2p, &pool, &storage).await
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

const DOWNLOAD_HTML: &str = r##"
<div class=card><h2>Download OpenCoin</h2>
<p>Build from source or download pre-built binaries.</p></div>
<div class=card><h3>Source Code</h3>
<pre>git clone https://github.com/mgamboa/opencoin.git
cd opencoin
cargo build --release
sudo cp target/release/opencoin-{node,wallet,miner} /usr/local/bin/</pre>
<p>Requires Rust: <code>curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></p>
</div>
<div class=card><h3>Quick Start (Join Network)</h3>
<pre># Start a node that connects to the seed:
opencoin-node start --seed mail.laat.com.au:9768

# Start a mining node (earns block rewards):
opencoin-node start --seed mail.laat.com.au:9768 --mine --premine-key YOUR-SECRET-KEY

# Connect miner to pool:
opencoin-miner --pool 192.168.2.10:3333 --address YOUR-OC-ADDRESS

# Create a wallet:
opencoin-wallet create --name my-wallet</pre>
</div>
<div class=card><h3>Seed Node</h3>
<table>
<tr><td>P2P</td><td><code>mail.laat.com.au:9768</code></td></tr>
<tr><td>RPC (Web)</td><td><code>http://mail.laat.com.au:9769</code></td></tr>
<tr><td>Pool</td><td><code>192.168.2.10:3333</code></td></tr>
</table>
</div>
"##;

fn html_page(title: &str, content: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        format!(
            r##"<!DOCTYPE html>
<html><head><title>{}</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:'Courier New',monospace;background:#0a0a0a;color:#0f0;padding:20px}}
h1{{color:#0f0;border-bottom:1px solid #0f0;padding-bottom:10px;margin-bottom:20px}}
h2{{color:#0f0;margin:15px 0 10px}}
a{{color:#0f0;text-decoration:none;margin-right:15px}}
a:hover{{text-decoration:underline}}
.nav{{margin-bottom:20px;padding:10px 0;border-bottom:1px solid #333}}
.nav a{{color:#0f0;font-size:16px;margin-right:20px}}
.card{{background:#111;border:1px solid #333;padding:15px;margin:10px 0;border-radius:5px}}
pre{{font-size:13px;overflow-x:auto}}
table{{width:100%;border-collapse:collapse;margin:10px 0}}
th,td{{text-align:left;padding:8px;border-bottom:1px solid #333}}
th{{color:#0a0}}
.status{{color:#0f0;font-weight:bold}}
.error{{color:#f00}}
</style></head><body>
<div class=nav>
<a href="/">Dashboard</a>
<a href="/wallet">Wallet</a>
<a href="/blocks">Blocks</a>
<a href="/pool">Pool</a>
<a href="/download">Download</a>
</div>
<h1>{}</h1>
{}</body></html>"##,
            title, title, content
        )
    )
}

async fn dashboard_html(blockchain: &Arc<RwLock<Blockchain>>, p2p: &Arc<P2PNetwork>) -> String {
    let bc = blockchain.read().await;
    let peers = p2p.peers.read().await;
    let peer_count = peers.len();
    format!(
        r##"<div class=card><h2>Blockchain</h2>
<table>
<tr><td>Height</td><td id=height>{}</td></tr>
<tr><td>Circulating Supply</td><td id=supply>{} OC</td></tr>
<tr><td>Total Work</td><td id=work>{}</td></tr>
<tr><td>Peers</td><td id=peers>{}</td></tr>
<tr><td>Version</td><td>{}</td></tr>
</table></div>
<div class=card><h2>Live</h2>
<pre id=live>Loading...</pre></div>
<script>
async function refresh(){{
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getinfo',params:[],id:1}})}});
let d=await r.json();
document.getElementById('height').textContent=d.result.height;
document.getElementById('supply').textContent=d.result.circulating_supply+' OC';
document.getElementById('work').textContent=d.result.total_work;
}}catch(e){{}}
document.getElementById('live').textContent=new Date().toISOString();
}}
setInterval(refresh,2000);refresh();
</script>"##,
        bc.state.height,
        bc.state.circulating_supply,
        bc.state.total_work,
        peer_count,
        crate::config::VERSION,
    )
}

async fn wallet_html(wallet: &Arc<RwLock<Option<Wallet>>>) -> String {
    let w = wallet.read().await;
    match w.as_ref() {
        Some(wallet_data) => {
            let addr = wallet_data.address_string().unwrap_or_default();
            let bal = wallet_data.balance;
            let locked = wallet_data.locked_balance;
            format!(
                r##"<div class=card><h2>Wallet</h2>
<table>
<tr><td>Address</td><td style=font-size:11px;word-break:break-all>{addr}</td></tr>
<tr><td>Balance</td><td id=balance>{bal} OC</td></tr>
<tr><td>Locked</td><td id=locked>{locked} OC</td></tr>
</table></div>
<div class=card><h2>Send Coins</h2>
<form id=sendForm onsubmit="sendCoins(event)">
<table>
<tr><td>Recipient Address:</td><td><input id=toAddress style="width:100%;font-family:monospace" placeholder="OC..."></td></tr>
<tr><td>Amount (OC):</td><td><input id=sendAmount type=number step=1 min=1 style="width:100%"></td></tr>
<tr><td></td><td><button type=submit style="background:#0f0;color:#000;border:none;padding:8px 20px;cursor:pointer;font-weight:bold;border-radius:4px">SEND</button></td></tr>
</table>
</form>
<pre id=sendResult style="margin-top:10px;color:#0f0"></pre>
</div>
<script>
async function refresh(){{
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getbalance',params:[],id:1}})}});
let d=await r.json();
document.getElementById('balance').textContent=d.result.balance+' OC';
document.getElementById('locked').textContent=d.result.locked+' OC';
}}catch(e){{}}
}}
async function sendCoins(e){{
e.preventDefault();
const addr=document.getElementById('toAddress').value.trim();
const amt=parseInt(document.getElementById('sendAmount').value);
if(!addr||!amt){{document.getElementById('sendResult').textContent='Fill in all fields';return;}}
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'sendtoaddress',params:[addr,amt],id:1}})}});
let d=await r.json();
if(d.result.error){{
document.getElementById('sendResult').textContent='Error: '+d.result.error;
}}else{{
document.getElementById('sendResult').innerHTML='✅ Sent '+amt+' OC<br>TX: <span style=font-size:11px>'+d.result.tx_hash+'</span>';
document.getElementById('sendAmount').value='';
document.getElementById('toAddress').value='';
refresh();
}}
}}catch(e){{document.getElementById('sendResult').textContent='Request failed: '+e;}}
}}
setInterval(refresh,2000);refresh();
</script>"##
            )
        }
        None => {
            "<div class=card><h2>Wallet</h2><p>No wallet loaded. Start node with --premine-key to load wallet.</p></div>".to_string()
        }
    }
}

async fn blocks_html(blockchain: &Arc<RwLock<Blockchain>>) -> String {
    let bc = blockchain.read().await;
    let height = bc.state.height;
    let start = if height > 20 { height - 20 } else { 0 };
    let mut rows = String::new();
    for h in (start..=height).rev() {
        if let Some(block) = bc.get_block(h) {
            let hash = hex::encode(block.hash());
            let short_hash = &hash[..16];
            rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                h,
                short_hash,
                block.transactions.len(),
                block.header.timestamp,
            ));
        }
    }
    format!(
        r##"<div class=card><h2>Recent Blocks (last 20)</h2>
<table>
<tr><th>Height</th><th>Hash</th><th>TXs</th><th>Timestamp</th></tr>
{rows}</table></div>"##
    )
}

async fn pool_html(pool: &Option<Arc<PoolServer>>) -> String {
    match pool {
        Some(p) => {
            let stats = p.stats().await;
            let miners = stats["miners"].as_u64().unwrap_or(0);
            let total_shares = stats["total_shares"].as_u64().unwrap_or(0);
            let job = &stats["current_job"];
            let miner_list = stats["miner_list"].as_array().cloned().unwrap_or_default();
            let mut miner_rows = String::new();
            for m in miner_list {
                miner_rows.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                    m["address"].as_str().unwrap_or(""),
                    m["shares"].as_u64().unwrap_or(0),
                    m["blocks_found"].as_u64().unwrap_or(0),
                ));
            }
            format!(
                r##"<div class=card><h2>Pool Status</h2>
<table>
<tr><td>Port</td><td>{port}</td></tr>
<tr><td>Connected Miners</td><td id=miners>{miner_count}</td></tr>
<tr><td>Total Shares</td><td id=shares>{shares}</td></tr>
<tr><td>Current Job</td><td>Height {height} / Job #{job_id}</td></tr>
<tr><td>Block Target</td><td>{block_target}</td></tr>
<tr><td>Share Target</td><td>{share_target}</td></tr>
</table></div>
<div class=card><h2>Miners ({miner_count})</h2>
<table>
<tr><th>Address</th><th>Shares</th><th>Blocks Found</th></tr>
{miner_rows}</table></div>
<script>
async function refresh(){{
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getpoolstats',params:[],id:1}})}});
let d=await r.json();
document.getElementById('miners').textContent=d.result.miners;
document.getElementById('shares').textContent=d.result.total_shares;
}}catch(e){{}}
}}
setInterval(refresh,2000);refresh();
</script>"##,
                port = stats["port"].as_u64().unwrap_or(0),
                miner_count = miners,
                shares = total_shares,
                height = job["height"].as_u64().unwrap_or(0),
                job_id = job["job_id"].as_u64().unwrap_or(0),
                block_target = job["block_target"].as_u64().unwrap_or(0),
                share_target = job["share_target"].as_u64().unwrap_or(0),
                miner_rows = if miner_rows.is_empty() { "<tr><td colspan=3>No miners connected</td></tr>".to_string() } else { miner_rows },
            )
        }
        None => {
            "<div class=card><h2>Pool</h2><p>Pool server not enabled. Start with <code>--pool</code> flag.</p></div>".to_string()
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
    pool: &Option<Arc<PoolServer>>,
    storage: &Option<Arc<Mutex<Storage>>>,
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
        "sendtoaddress" => {
            let to_address = params.get(0).and_then(|p| p.as_str()).unwrap_or("").to_string();
            let amount = params.get(1).and_then(|p| p.as_u64()).unwrap_or(0);
            let fee = params.get(2).and_then(|p| p.as_u64()).unwrap_or(crate::config::COIN / 1000);

            if to_address.is_empty() || amount == 0 {
                serde_json::json!({"error": "Invalid parameters: need address and amount"})
            } else if let Ok(oc_addr) = crate::chain::address::OpenCoinAddress::from_string(&to_address) {
                let w = wallet.read().await;
                if let Some(w_ref) = w.as_ref() {
                    if amount + fee > w_ref.balance {
                        serde_json::json!({"error": "Insufficient balance"})
                    } else {
                        let sender_stealth = w_ref.stealth_address().unwrap();
                        let recipient_stealth = oc_addr.to_stealth();
                        let change = w_ref.balance - amount - fee;
                        let tx = crate::chain::transaction::Transaction::transfer(
                            &sender_stealth, &recipient_stealth, amount, fee, change
                        );
                        let tx_hash = tx.hash();
                        drop(w);

                        let mut w = wallet.write().await;
                        if let Some(ref mut wallet) = *w {
                            wallet.balance = wallet.balance.saturating_sub(amount + fee);
                            wallet.transactions.push(tx_hash);
                        }
                        drop(w);

                        p2p.broadcast_transaction(&tx).await;
                        let _ = p2p.add_to_mempool(tx.clone()).await;

                        if let Some(ref st) = storage {
                            if let Ok(s) = st.lock() {
                                let _ = s.save_transaction(&tx);
                                let _ = s.flush();
                            }
                        }

                        serde_json::json!({
                            "tx_hash": hex::encode(tx_hash),
                            "amount": amount,
                            "fee": fee,
                            "to": to_address,
                            "change": change,
                        })
                    }
                } else {
                    serde_json::json!({"error": "No wallet loaded"})
                }
            } else {
                serde_json::json!({"error": format!("Invalid address: {}", to_address)})
            }
        }
        "getpoolstats" => {
            match pool {
                Some(p) => p.stats().await,
                None => serde_json::json!({"error": "Pool not enabled"}),
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
