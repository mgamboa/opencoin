use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::RwLock;
use chrono::{DateTime, FixedOffset};

use crate::chain::blockchain::Blockchain;
use crate::chain::transaction::Transaction;
use crate::wallet::Wallet;
use crate::p2p::P2PNetwork;
use crate::pool::PoolServer;
use crate::storage::db::Storage;
use crate::vm::wasm::{WasmRuntime, ContractContext};

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
        ("GET", "/explorer") => explorer_page(&blockchain, &p2p, None, &storage).await,
        ("GET", "/faucet") => html_page("OpenCoin Faucet", &faucet_html(&wallet).await),
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
        _ if path.starts_with("/explorer/block/") => {
            let height_str = path.trim_start_matches("/explorer/block/").split('/').next().unwrap_or("");
            let h = height_str.parse::<u64>().ok();
            explorer_page(&blockchain, &p2p, h, &storage).await
        }
        _ if path.starts_with("/explorer/tx/") => {
            let tx_hex = path.trim_start_matches("/explorer/tx/").split('/').next().unwrap_or("");
            tx_detail_page(&blockchain, tx_hex, &storage).await
        }
        ("POST", "/faucet") => {
            let to_addr = body.lines().next().unwrap_or("").trim().to_string();
            faucet_send(&wallet, &to_addr).await
        }
        ("GET", "/pwa") | ("GET", "/app") | ("GET", "/wallet-app") => {
            "HTTP/1.1 302 Found\r\nLocation: http://144.6.203.69:9770/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        }
        _ => {
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"not found\"}".to_string()
        }
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

fn format_timestamp(ts: u64) -> String {
    let aest = FixedOffset::east_opt(10 * 3600).unwrap_or(FixedOffset::east_opt(0).unwrap());
    let utc_dt = DateTime::from_timestamp(ts as i64, 0).unwrap_or_default();
    let dt = utc_dt.with_timezone(&aest);
    dt.format("%d/%m/%Y %I:%M:%S %p AEST").to_string()
}

const DOWNLOAD_HTML: &str = r##"
<div class=card><h2>Download OpenCoin</h2>
<p>Build from source or download the mobile wallet APK.</p></div>
<div class=card><h3>Mobile Wallet (Android APK)</h3>
<a class=btn href="//144.6.203.69:9770/OpenCoin-Wallet-v1.0.0.apk" download>Download OpenCoin Wallet v1.0.0 (APK, 79 MB)</a>
<p style="margin-top:8px;color:#8b949e;font-size:13px">OpenCoin Wallet for Android. Install the APK on your device to send, receive, and monitor your balance.</p></div>
<div class=card><h3>Source Code</h3>
<pre>git clone https://github.com/mgamboa/opencoin.git
cd opencoin
cargo build --release
sudo cp target/release/opencoin-{node,wallet,miner} /usr/local/bin/</pre>
<p>Requires Rust: <code>curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></p>
</div>
<div class=card><h3>Quick Start (Join Network)</h3>
<pre># Start a relay node (bootstraps from peers.json):
opencoin-node start

# Start a mining node with your wallet:
opencoin-node start --mine --premine-key YOUR-SECRET-KEY

# Discover a live pool and mine:
opencoin-miner --discover --address YOUR-OC-ADDRESS --threads 4

# Or connect to a specific pool:
opencoin-miner --pool host:3333 --address YOUR-OC-ADDRESS --threads 4

# Create a wallet:
opencoin-wallet create --name my-wallet</pre>
</div>
<div class=card><h3>Create a Wallet</h3>
<pre># Generate a new wallet:
opencoin-wallet create --name my-wallet
# Output: Address, Public key, Secret key

# View your wallet:
opencoin-wallet show --name my-wallet

# Generate just a keypair (for --premine-key or --pool-address):
opencoin-wallet generate-key</pre>
<p>Your wallet address looks like: <code style="font-size:11px">OC2141d609ed887915acc59ceacdd3f6ca7ff9b8a480ea1597bcb1a3d0fb3e6ea4ba6622fb</code></p>
</div>
<div class=card><h3>Wallet Recovery</h3>
<p>Your wallet <strong>IS your secret key</strong>. Lose the machine, not the key.</p>
<pre># Recover wallet from secret key:
opencoin-node start --premine-key YOUR-SECRET-HEX-KEY

# View your address and balance:
opencoin-wallet show --name my-wallet

# Generate a fresh keypair (SAVE THE SECRET KEY):
opencoin-wallet generate-key</pre>
<p><strong>⚠️ Save your secret key offline.</strong> Without it, your coins are gone forever.</p>
</div>
<div class=card><h3>Seed Node</h3>
<table>
<tr><td>P2P</td><td><code>any-public-node:9768</code></td></tr>
<tr><td>RPC (Web)</td><td><code>http://any-public-node:9769</code></td></tr>
<tr><td>Pool</td><td><code>any-public-node:3333</code></td></tr>
</table>
<p>See <code>peers.json</code> in the repo for live public nodes.</p>
</div>
"##;

fn html_page(title: &str, content: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        format!(
            r##"<!DOCTYPE html>
<html><head><title>{}</title>
<meta charset="utf-8">
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
<a href="/explorer">Explorer</a>
<a href="/wallet">Wallet</a>
<a href="/blocks">Blocks</a>
<a href="/pool">Pool</a>
<a href="/faucet">Faucet</a>
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
    let supply_units = bc.state.circulating_supply;
    format!(
        r##"<div class=card style="text-align:center;border-color:#0f0;background:#0a1a0a">
<h2 style=font-size:28px;margin:0>OpenCoin</h2>
<p style=color:#0a0;margin:10px 0>A next-generation cryptocurrency with privacy, smart contracts, and SPV light clients.</p>
<div style=margin:20px 0>
<a href=/explorer style="display:inline-block;background:#0f0;color:#000;padding:10px 25px;margin:5px;border-radius:4px;font-weight:bold">Explorer</a>
<a href=/faucet style="display:inline-block;background:#0f0;color:#000;padding:10px 25px;margin:5px;border-radius:4px;font-weight:bold">Faucet</a>
<a href=/download style="display:inline-block;background:#222;color:#0f0;padding:10px 25px;margin:5px;border-radius:4px;border:1px solid #0f0">Download</a>
</div>
</div>
<div class=card><h2>Network</h2>
<table>
<tr><td>Height</td><td id=height>{height}</td></tr>
<tr><td>Circulating Supply</td><td id=supply>{supply} units (20M premined)</td></tr>
<tr><td>Peers</td><td id=peers>{peers}</td></tr>
<tr><td>Version</td><td>{version}</td></tr>
<tr><td>Block Time</td><td>120 seconds (target)</td></tr>
</table></div>
<div class=card><h2>Features</h2>
<table>
<tr><td>🔗</td><td><strong>Proof of Work</strong></td><td>CPU-mineable SHA3-256 based PoW</td></tr>
<tr><td>🛡️</td><td><strong>Stealth Addresses</strong></td><td>One-time output keys for privacy</td></tr>
<tr><td>🔒</td><td><strong>RingCT</strong></td><td>Optional confidential transactions</td></tr>
<tr><td>📜</td><td><strong>Smart Contracts</strong></td><td>WASM-based contract execution</td></tr>
<tr><td>📱</td><td><strong>Light Client</strong></td><td>SPV verification with bloom filters</td></tr>
<tr><td>⚡</td><td><strong>Mining Pool</strong></td><td>Built-in pool server with real-time miner dashboard</td></tr>
</table></div>
<div class=card><h2>Latest Block</h2>
<pre id=live>Loading...</pre></div>
<script>
async function refresh(){{
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getinfo',params:[],id:1}})}});
let d=await r.json();
document.getElementById('height').textContent=d.result.height;
document.getElementById('supply').textContent=d.result.circulating_supply+' units';
}}catch(e){{}}
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getblock',params:[parseInt(document.getElementById('height').textContent)],id:1}})}});
let d=await r.json();
document.getElementById('live').innerHTML='Height: '+d.result.height+' | TXs: '+d.result.tx_count+' | Hash: '+d.result.hash.slice(0,32)+'...';
}}catch(e){{document.getElementById('live').textContent=new Date().toISOString();}}
}}
setInterval(refresh,3000);refresh();
</script>"##,
        height = bc.state.height,
        supply = supply_units,
        peers = peer_count,
        version = crate::config::VERSION,
    )
}

async fn wallet_html(wallet: &Arc<RwLock<Option<Wallet>>>) -> String {
    let w = wallet.read().await;
    match w.as_ref() {
        Some(wallet_data) => {
            let addr = wallet_data.address_string().unwrap_or_default();
            let bal = wallet_data.balance;
            let locked = wallet_data.locked_balance;
            let utxo_count = wallet_data.utxos.len();
            format!(
                r##"<div class=card><h2>Wallet</h2>
<table>
<tr><td>Address</td><td style=font-size:11px;word-break:break-all>{addr}</td></tr>
<tr><td>Balance</td><td id=balance>{bal} OC</td></tr>
<tr><td>Locked</td><td id=locked>{locked} OC</td></tr>
<tr><td>UTXOs</td><td>{utxo_count}</td></tr>
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
    let mut tx_detail = String::new();
    for h in (start..=height).rev() {
        if let Some(block) = bc.get_block(h) {
            let hash = hex::encode(block.hash());
            let short_hash = &hash[..16];
            let (reward, recipient, tx_count) = block.transactions.first()
                .map(|tx| {
                    let total: u64 = tx.outputs.iter().map(|o| o.amount).sum();
                    let addr = tx.outputs.first()
                        .map(|o| {
                            let hex_addr = hex::encode(&o.stealth_address.spend_pub.0[..8]);
                            format!("{}...", hex_addr)
                        })
                        .unwrap_or_default();
                    (total, addr, block.transactions.len())
                })
                .unwrap_or((0, String::new(), 0));
            let ts_str = format_timestamp(block.header.timestamp);
            rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                h, short_hash, reward, recipient, tx_count, ts_str,
            ));
            if h == height {
                for (ti, tx) in block.transactions.iter().enumerate() {
                    let txh = hex::encode(tx.hash());
                    tx_detail.push_str(&format!(
                        "<div class=card style=font-size:12px><b>TX #{}:</b> {}<br>",
                        ti, &txh[..32]
                    ));
                    for (oi, output) in tx.outputs.iter().enumerate() {
                        let addr = hex::encode(&output.stealth_address.spend_pub.0[..8]);
                        tx_detail.push_str(&format!("  Output {}: {} OC → {}...<br>", oi, output.amount, addr));
                    }
                    if !tx.inputs.is_empty() {
                        for (ii, input) in tx.inputs.iter().enumerate() {
                            let prev = hex::encode(&input.outpoint.tx_hash[..8]);
                            tx_detail.push_str(&format!("  Input {}: from {}:{}<br>", ii, prev, input.outpoint.index));
                        }
                    }
                    tx_detail.push_str("</div>");
                }
            }
        }
    }
    format!(
        r##"<div class=card><h2>Recent Blocks (last 20)</h2>
<table>
<tr><th>Height</th><th>Hash</th><th>Reward</th><th>Miner</th><th>TXs</th><th>Timestamp</th></tr>
{rows}</table></div>
<h2>Latest Block Details</h2>
{tx_detail}"##
    )
}

async fn pool_html(pool: &Option<Arc<PoolServer>>) -> String {
    match pool {
        Some(p) => {
            let stats = p.stats().await;
            let miners = stats["miners"].as_u64().unwrap_or(0);
            let total_shares = stats["total_shares"].as_u64().unwrap_or(0);
            let job = &stats["current_job"];
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let miner_list = stats["miner_list"].as_array().cloned().unwrap_or_default();
            let mut miner_rows = String::new();
            for m in miner_list {
                let last_share = m["last_share"].as_u64().unwrap_or(0);
                let secs_ago = now.saturating_sub(last_share);
                let status = if secs_ago < 30 { "🟢" } else if secs_ago < 120 { "🟡" } else { "🔴" };
                let wallet = m["wallet"].as_str().unwrap_or("").to_string();
                let wallet_short = if wallet.len() > 16 { format!("{}...", &wallet[..16]) } else { wallet };
                miner_rows.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}s ago</td></tr>",
                    status,
                    wallet_short,
                    m["shares"].as_u64().unwrap_or(0),
                    m["blocks_found"].as_u64().unwrap_or(0),
                    secs_ago,
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
<tr><th>Status</th><th>Address</th><th>Shares</th><th>Blocks</th><th>Last Share</th></tr>
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

async fn explorer_page(
    blockchain: &Arc<RwLock<Blockchain>>,
    p2p: &Arc<P2PNetwork>,
    block_height: Option<u64>,
    _storage: &Option<Arc<Mutex<Storage>>>,
) -> String {
    let bc = blockchain.read().await;
    let height = bc.state.height;
    let peers = p2p.peers.read().await.len();

    if let Some(h) = block_height {
        if let Some(block) = bc.get_block(h) {
            let hash = hex::encode(block.hash());
            let prev_hash = hex::encode(block.header.previous_hash);
            let merkle = hex::encode(block.header.merkle_root);
            let mut tx_rows = String::new();
            for (i, tx) in block.transactions.iter().enumerate() {
                let txh = hex::encode(tx.hash());
                let tx_type = format!("{:?}", tx.tx_type);
                let total = tx.total_output();
                tx_rows.push_str(&format!(
                    r#"<tr><td style=font-size:11px><a href="/explorer/tx/{}">{}</a></td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                    &txh[..32], &txh[..32], tx_type, total, tx.inputs.len(),
                ));
            }
            let ts = format_timestamp(block.header.timestamp);
            let reward = block.transactions.first()
                .map(|tx| tx.outputs.iter().map(|o| o.amount).sum::<u64>())
                .unwrap_or(0);
            let content = format!(
                r##"<div class=card>
<h2>Block #{h}</h2>
<table>
<tr><td>Hash</td><td style=font-size:11px>{hash}</td></tr>
<tr><td>Previous Hash</td><td style=font-size:11px>{prev_hash}</td></tr>
<tr><td>Merkle Root</td><td style=font-size:11px>{merkle}</td></tr>
<tr><td>Timestamp</td><td>{ts}</td></tr>
<tr><td>Transactions</td><td>{tx_count}</td></tr>
<tr><td>Reward</td><td>{reward} OC</td></tr>
<tr><td>Difficulty</td><td>{diff}</td></tr>
<tr><td>Nonce</td><td>{nonce}</td></tr>
</table></div>
<div class=card><h2>Transactions ({tx_count})</h2>
<table>
<tr><th>Hash</th><th>Type</th><th>Total</th><th>Inputs</th></tr>
{tx_rows}</table></div>
<div style=margin-top:10px>
<a href="/explorer/block/{prev}">← Previous Block</a>
<a href="/explorer/block/{next}" style=margin-left:20px>Next Block →</a>
</div>"##,
                h = h, hash = hash, prev_hash = prev_hash, merkle = merkle,
                ts = ts, tx_count = block.transactions.len(), reward = reward,
                diff = block.header.difficulty_target, nonce = block.header.nonce,
                tx_rows = tx_rows,
                prev = h.saturating_sub(1), next = h + 1,
            );
            return html_page(&format!("Block #{}", h), &content);
        }
        return html_page("Block Not Found", "<div class=card><h2>Block Not Found</h2><p>No block at this height.</p><a href=/explorer>← Back to Explorer</a></div>");
    }

    let start = if height > 25 { height - 25 } else { 0 };
    let mut rows = String::new();
    for h in (start..=height).rev() {
        if let Some(block) = bc.get_block(h) {
            let hash = hex::encode(block.hash());
            let reward = block.transactions.first()
                .map(|tx| tx.outputs.iter().map(|o| o.amount).sum::<u64>())
                .unwrap_or(0);
            let ts = format_timestamp(block.header.timestamp);
            rows.push_str(&format!(
                r#"<tr><td>{}</td><td style=font-size:11px><a href="/explorer/block/{}">{}</a></td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                h, h, &hash[..16], reward, block.transactions.len(), ts,
            ));
        }
    }
    let content = format!(
        r##"<div class=card><h2>Network</h2>
<table>
<tr><td>Height</td><td id=exp-height>{height}</td></tr>
<tr><td>Circulating Supply</td><td id=exp-supply>{supply} OC</td></tr>
<tr><td>Peers</td><td id=exp-peers>{peers}</td></tr>
</table></div>
<div class=card><h2>Search</h2>
<form onsubmit="searchExplorer(event)" style=display:flex;gap:10px>
<input id=searchQ style="flex:1;padding:8px;background:#222;color:#0f0;border:1px solid #333;border-radius:4px;font-family:monospace" placeholder="Block height, transaction hash, or address...">
<button type=submit style="background:#0f0;color:#000;border:none;padding:8px 20px;border-radius:4px;font-weight:bold;cursor:pointer">Search</button>
</form>
<pre id=searchResult style=margin-top:10px></pre>
</div>
<div class=card><h2>Recent Blocks (last 25)</h2>
<table>
<tr><th>Height</th><th>Hash</th><th>Reward</th><th>TXs</th><th>Timestamp</th></tr>
{rows}</table></div>
<script>
function searchExplorer(e){{e.preventDefault();
let q=document.getElementById('searchQ').value.trim();
if(!q)return;
// Try as block height
let h=parseInt(q);
if(!isNaN(h)&&h>=0){{window.location='/explorer/block/'+h;return;}}
// Try as tx hash
if(q.length>=16){{window.location='/explorer/tx/'+q;return;}}
document.getElementById('searchResult').textContent='No results for: '+q;
}}
async function refresh(){{
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'getinfo',params:[],id:1}})}});
let d=await r.json();
document.getElementById('exp-height').textContent=d.result.height;
document.getElementById('exp-supply').textContent=d.result.circulating_supply+' OC';
document.getElementById('exp-peers').textContent=d.result.peers||'?';
}}catch(e){{}}
}}
setInterval(refresh,3000);refresh();
</script>"##,
        height = height, supply = bc.state.circulating_supply, peers = peers,
    );
    html_page("Explorer", &content)
}

async fn tx_detail_page(
    blockchain: &Arc<RwLock<Blockchain>>,
    tx_hex: &str,
    storage: &Option<Arc<Mutex<Storage>>>,
) -> String {
    let tx_hash_hex = tx_hex.trim();
    let tx_hash_bytes = match hex::decode(tx_hash_hex) {
        Ok(b) if b.len() >= 32 => {
            let mut h = [0u8; 32];
            let start = if b.len() > 32 { b.len() - 32 } else { 0 };
            h.copy_from_slice(&b[start..start+32]);
            h
        }
        _ => {
            // Try looking up by scanning blocks
            let mut found = None;
            let bc = blockchain.read().await;
            'outer: for h in 0..=bc.state.height {
                if let Some(block) = bc.get_block(h) {
                    for tx in &block.transactions {
                        if hex::encode(tx.hash()).contains(tx_hash_hex) || hex::encode(tx.hash()).starts_with(tx_hash_hex) {
                            found = Some((tx.clone(), h));
                            break 'outer;
                        }
                    }
                }
            }
            match found {
                Some((tx, height)) => return tx_detail_render(&tx, height, tx_hash_hex),
                None => return html_page("Transaction Not Found", &format!(
                    "<div class=card><h2>Transaction Not Found</h2><p>No transaction matching '{}'.</p><a href=/explorer>← Back to Explorer</a></div>", tx_hash_hex
                )),
            }
        }
    };

    // Look up by hash from storage first
    if let Some(ref st) = storage {
        let found = st.lock().ok().and_then(|s| s.get_transaction(&tx_hash_bytes).ok().flatten());
        if let Some(tx) = found {
            // Find which block contains this tx
            let bc = blockchain.read().await;
            for h in 0..=bc.state.height {
                if let Some(block) = bc.get_block(h) {
                    if block.transactions.iter().any(|t| t.hash() == tx_hash_bytes) {
                        return tx_detail_render(&tx, h, tx_hash_hex);
                    }
                }
            }
            return tx_detail_render(&tx, 0, tx_hash_hex);
        }
    }

    // Fallback: scan blocks in memory
    let bc = blockchain.read().await;
    for h in 0..=bc.state.height {
        if let Some(block) = bc.get_block(h) {
            if let Some(tx) = block.transactions.iter().find(|t| t.hash() == tx_hash_bytes) {
                return tx_detail_render(tx, h, tx_hash_hex);
            }
        }
    }
    html_page("Transaction Not Found", &format!(
        "<div class=card><h2>Transaction Not Found</h2><p>No transaction matching '{}'.</p><a href=/explorer>← Back to Explorer</a></div>", tx_hash_hex
    ))
}

fn tx_detail_render(tx: &Transaction, height: u64, tx_hash_hex: &str) -> String {
    let txh = hex::encode(tx.hash());
    let tx_type = format!("{:?}", tx.tx_type);
    let ts = format_timestamp(tx.timestamp);
    let mut outputs_html = String::new();
    for (i, o) in tx.outputs.iter().enumerate() {
        let addr = hex::encode(&o.stealth_address.spend_pub.0);
        outputs_html.push_str(&format!(
            "<tr><td>{}</td><td style=font-size:11px>{}</td><td>{}</td></tr>",
            i, addr, o.amount,
        ));
    }
    let mut inputs_html = String::new();
    for (i, inp) in tx.inputs.iter().enumerate() {
        let prev_tx = hex::encode(&inp.outpoint.tx_hash);
        inputs_html.push_str(&format!(
            "<tr><td>{}</td><td style=font-size:11px>{}</td><td>{}</td></tr>",
            i, prev_tx, inp.outpoint.index,
        ));
    }
    let content = format!(
        r##"<div class=card><h2>Transaction</h2>
<table>
<tr><td>Hash</td><td style=font-size:11px;word-break:break-all>{txh}</td></tr>
<tr><td>Type</td><td>{tx_type}</td></tr>
<tr><td>Fee</td><td>{fee}</td></tr>
<tr><td>Timestamp</td><td>{ts}</td></tr>
<tr><td>Block Height</td><td><a href="/explorer/block/{height}">{height}</a></td></tr>
</table></div>
<div class=card><h2>Outputs ({outputs_len})</h2>
<table><tr><th>Index</th><th>Address</th><th>Amount</th></tr>{outputs_html}</table></div>
<div class=card><h2>Inputs ({inputs_len})</h2>
<table><tr><th>Index</th><th>Source TX</th><th>Output</th></tr>{inputs_html}</table></div>
<div style=margin-top:10px><a href="/explorer">← Back to Explorer</a></div>"##,
        txh = txh, tx_type = tx_type, fee = tx.fee, ts = ts, height = height,
        outputs_len = tx.outputs.len(), outputs_html = outputs_html,
        inputs_len = tx.inputs.len(), inputs_html = inputs_html,
    );
    html_page(&format!("TX {}", &tx_hash_hex[..16]), &content)
}

async fn faucet_html(wallet: &Arc<RwLock<Option<Wallet>>>) -> String {
    let w = wallet.read().await;
    match w.as_ref() {
        Some(wallet_data) => {
            let addr = wallet_data.address_string().unwrap_or_default();
            let bal = wallet_data.balance;
            format!(
                r##"<div class=card><h2>OpenCoin Faucet</h2>
<p>Get free test coins to try the network. Limited to 100 OC per request.</p>
<table>
<tr><td>Faucet Balance</td><td id=faucet-bal>{bal} OC</td></tr>
<tr><td>Max per request</td><td>100 OC</td></tr>
</table></div>
<div class=card><h2>Request Coins</h2>
<form onsubmit="faucetSend(event)">
<table>
<tr><td>Your Address:</td><td><input id=faucetAddr style="width:100%;font-family:monospace;background:#222;color:#0f0;border:1px solid #333;padding:8px;border-radius:4px" placeholder="OC..."></td></tr>
<tr><td></td><td><button type=submit style="background:#0f0;color:#000;border:none;padding:8px 20px;border-radius:4px;font-weight:bold;cursor:pointer">GET COINS</button></td></tr>
</table>
</form>
<pre id=faucetResult style=margin-top:10px></pre>
</div>
<script>
async function faucetSend(e){{e.preventDefault();
const addr=document.getElementById('faucetAddr').value.trim();
if(!addr){{document.getElementById('faucetResult').textContent='Enter an address';return;}}
try{{
let r=await fetch('/rpc',{{method:'POST',headers:{{'Content-Type':'application/json'}},
body:JSON.stringify({{jsonrpc:'2.0',method:'sendtoaddress',params:[addr,100],id:1}})}});
let d=await r.json();
if(d.result.error){{
document.getElementById('faucetResult').textContent='Error: '+d.result.error;
}}else{{
document.getElementById('faucetResult').innerHTML='✅ Sent 100 OC!<br>TX: <span style=font-size:11px>'+d.result.tx_hash+'</span>';
document.getElementById('faucet-bal').textContent=(parseInt(document.getElementById('faucet-bal').textContent)-100)+' OC';
}}
}}catch(e){{document.getElementById('faucetResult').textContent='Request failed: '+e;}}
}}
</script>"##
            )
        }
        None => {
            r#"<div class=card><h2>Faucet</h2><p>Faucet is not available right now.</p></div>"#.to_string()
        }
    }
}

async fn faucet_send(wallet: &Arc<RwLock<Option<Wallet>>>, to_addr: &str) -> String {
    let w = wallet.read().await;
    match w.as_ref() {
        Some(w_ref) => {
            let bal = w_ref.balance;
            if bal < 100 {
                return "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><p>Faucet empty. Balance: {bal} OC</p><a href=/faucet>← Back</a></body></html>".to_string();
            }
            if to_addr.is_empty() || !to_addr.starts_with("OC") || to_addr.len() != 74 {
                return "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><p>Invalid address. Must be a valid OC address (74 chars starting with OC).</p><a href=/faucet>← Back</a></body></html>".to_string();
            }
            // Send via RPC parameters - we'll redirect to the JS approach
            let url = format!("/faucet");
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n", url
            )
        }
        None => {
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><p>Faucet wallet not loaded.</p><a href=/faucet>← Back</a></body></html>".to_string()
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
            let fee = params.get(2).and_then(|p| p.as_u64()).unwrap_or(crate::config::COIN / 10000);

            if to_address.is_empty() || amount == 0 {
                serde_json::json!({"error": "Invalid parameters: need address and amount"})
            } else if let Ok(oc_addr) = crate::chain::address::OpenCoinAddress::from_string(&to_address) {
                let w = wallet.read().await;
                if let Some(w_ref) = w.as_ref() {
                    let needed = amount + fee;
                    if needed > w_ref.balance {
                        serde_json::json!({"error": "Insufficient balance"})
                    } else {
                        let sender_kp = w_ref.keypair().unwrap();
                        let recipient_stealth = oc_addr.to_stealth();
                        let utxos = w_ref.get_utxos_for_amount(needed);
                        drop(w);

                        let tx = crate::chain::transaction::Transaction::transfer(
                            &sender_kp, &recipient_stealth, amount, fee, &utxos
                        );
                        let tx_hash = tx.hash();

                        let mut w = wallet.write().await;
                        if let Some(ref mut wallet) = *w {
                            for (outpoint, _) in &utxos {
                                let key = format!("{}:{}", hex::encode(outpoint.tx_hash), outpoint.index);
                                wallet.utxos.remove(&key);
                            }
                            wallet.transactions.push(tx_hash);
                            wallet.balance = wallet.utxos.values().map(|(_, a)| a).sum();
                        }
                        drop(w);

                        if let Some(ref st) = storage {
                            let w = wallet.read().await;
                            if let Some(ref wlt) = *w {
                                if let Ok(s) = st.lock() {
                                    let _ = s.save_wallet(wlt);
                                    let _ = s.save_transaction(&tx);
                                    let _ = s.flush();
                                }
                            }
                        }

                        p2p.broadcast_transaction(&tx).await;
                        let _ = p2p.add_to_mempool(tx.clone()).await;

                        serde_json::json!({
                            "tx_hash": hex::encode(tx_hash),
                            "amount": amount,
                            "fee": fee,
                            "to": to_address,
                            "change": utxos.iter().map(|(_, a)| a).sum::<u64>() - amount - fee,
                        })
                    }
                } else {
                    serde_json::json!({"error": "No wallet loaded"})
                }
            } else {
                serde_json::json!({"error": format!("Invalid address: {}", to_address)})
            }
        }
        "getblockcount" => {
            serde_json::json!({"blocks": blockchain.read().await.state.height})
        }
        "getblockhash" => {
            let height = params.get(0).and_then(|p| p.as_u64()).unwrap_or(0);
            let bc = blockchain.read().await;
            match bc.get_block(height) {
                Some(block) => serde_json::json!({"hash": hex::encode(block.hash())}),
                None => serde_json::json!({"error": "Block not found"}),
            }
        }
        "getblockheader" => {
            let height = params.get(0).and_then(|p| p.as_u64()).unwrap_or(0);
            let bc = blockchain.read().await;
            match bc.get_block(height) {
                Some(block) => serde_json::json!({
                    "hash": hex::encode(block.hash()),
                    "height": block.header.height,
                    "version": block.header.version,
                    "timestamp": block.header.timestamp,
                    "previous_hash": hex::encode(block.header.previous_hash),
                    "merkle_root": hex::encode(block.header.merkle_root),
                    "difficulty": block.header.difficulty_target,
                    "nonce": block.header.nonce,
                    "tx_count": block.transactions.len(),
                }),
                None => serde_json::json!({"error": "Block not found"}),
            }
        }
        "gettransaction" => {
            let tx_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let tx_hash = match hex::decode(tx_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&b);
                    h
                }
                _ => return serde_json::json!({"error": "Invalid tx hash hex"}),
            };
            match storage {
                Some(ref st) => {
                    if let Ok(s) = st.lock() {
                        match s.get_transaction(&tx_hash) {
                            Ok(Some(tx)) => serde_json::json!({
                                "tx_hash": hex::encode(tx_hash),
                                "tx_type": format!("{:?}", tx.tx_type),
                                "fee": tx.fee,
                                "timestamp": tx.timestamp,
                                "inputs": tx.inputs.len(),
                                "outputs": tx.outputs.len(),
                                "total_output": tx.total_output(),
                            }),
                            _ => serde_json::json!({"error": "Transaction not found"}),
                        }
                    } else {
                        serde_json::json!({"error": "Storage error"})
                    }
                }
                None => serde_json::json!({"error": "No storage"}),
            }
        }
        "sendrawtransaction" => {
            let tx_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let tx_data = match hex::decode(tx_hex) {
                Ok(d) => d,
                Err(_) => return serde_json::json!({"error": "Invalid hex"}),
            };
            let tx: Transaction = match bincode::deserialize(&tx_data) {
                Ok(t) => t,
                Err(_) => return serde_json::json!({"error": "Invalid transaction data"}),
            };
            let tx_hash = tx.hash();
            p2p.broadcast_transaction(&tx).await;
            let _ = p2p.add_to_mempool(tx.clone()).await;
            if let Some(ref st) = storage {
                if let Ok(s) = st.lock() {
                    let _ = s.save_transaction(&tx);
                    let _ = s.flush();
                }
            }
            serde_json::json!({"tx_hash": hex::encode(tx_hash)})
        }
        "getrawtransaction" => {
            let tx_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let tx_hash = match hex::decode(tx_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&b);
                    h
                }
                _ => return serde_json::json!({"error": "Invalid tx hash hex"}),
            };
            match storage {
                Some(ref st) => {
                    if let Ok(s) = st.lock() {
                        match s.get_transaction(&tx_hash) {
                            Ok(Some(tx)) => {
                                let raw = bincode::serialize(&tx).unwrap_or_default();
                                serde_json::json!({"raw": hex::encode(raw)})
                            }
                            _ => serde_json::json!({"error": "Transaction not found"}),
                        }
                    } else {
                        serde_json::json!({"error": "Storage error"})
                    }
                }
                None => serde_json::json!({"error": "No storage"}),
            }
        }
        "validateaddress" => {
            let addr_str = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let is_valid = crate::chain::address::OpenCoinAddress::from_string(addr_str).is_ok();
            serde_json::json!({
                "address": addr_str,
                "is_valid": is_valid,
            })
        }
        "deploycontract" => {
            let code_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let args_hex = params.get(1).and_then(|p| p.as_str()).unwrap_or("");
            let fee = params.get(2).and_then(|p| p.as_u64()).unwrap_or(1000);
            let code = match hex::decode(code_hex) {
                Ok(c) => c,
                Err(_) => return serde_json::json!({"error": "Invalid code hex"}),
            };
            let args = match hex::decode(args_hex) {
                Ok(a) => a,
                Err(_) => return serde_json::json!({"error": "Invalid args hex"}),
            };
            let w = wallet.read().await;
            let w_ref = match w.as_ref() {
                Some(w) => w,
                None => return serde_json::json!({"error": "No wallet loaded"}),
            };
            let sender_kp = match w_ref.keypair() {
                Ok(kp) => kp,
                Err(_) => return serde_json::json!({"error": "Invalid wallet keypair"}),
            };
            let needed = fee;
            if needed > w_ref.balance {
                return serde_json::json!({"error": "Insufficient balance"});
            }
            let utxos = w_ref.get_utxos_for_amount(needed);
            drop(w);
            let tx = Transaction::contract_deploy(&sender_kp, code.clone(), args, fee, &utxos);
            let tx_hash = tx.hash();
            let addr = crate::vm::wasm::contract_address(&tx_hash, {
                blockchain.read().await.state.height + 1
            });
            let mut w = wallet.write().await;
            if let Some(ref mut wallet) = *w {
                for (outpoint, _) in &utxos {
                    let key = format!("{}:{}", hex::encode(outpoint.tx_hash), outpoint.index);
                    wallet.utxos.remove(&key);
                }
                wallet.transactions.push(tx_hash);
                wallet.balance = wallet.utxos.values().map(|(_, a)| a).sum();
            }
            drop(w);
            if let Some(ref st) = storage {
                let w = wallet.read().await;
                if let Some(ref wlt) = *w {
                    if let Ok(s) = st.lock() {
                        let _ = s.save_wallet(wlt);
                        let _ = s.save_transaction(&tx);
                        let _ = s.flush();
                    }
                }
            }
            p2p.broadcast_transaction(&tx).await;
            let _ = p2p.add_to_mempool(tx.clone()).await;
            serde_json::json!({
                "tx_hash": hex::encode(tx_hash),
                "contract_address": hex::encode(addr),
                "code_size": code.len(),
            })
        }
        "callcontract" => {
            let addr_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let function = params.get(1).and_then(|p| p.as_str()).unwrap_or("");
            let args_hex = params.get(2).and_then(|p| p.as_str()).unwrap_or("");
            let fee = params.get(3).and_then(|p| p.as_u64()).unwrap_or(1000);
            let contract_addr = match hex::decode(addr_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut a = [0u8; 32];
                    a.copy_from_slice(&b);
                    a
                }
                _ => return serde_json::json!({"error": "Invalid contract address"}),
            };
            let args = match hex::decode(args_hex) {
                Ok(a) => a,
                Err(_) => return serde_json::json!({"error": "Invalid args hex"}),
            };
            let w = wallet.read().await;
            let w_ref = match w.as_ref() {
                Some(w) => w,
                None => return serde_json::json!({"error": "No wallet loaded"}),
            };
            let sender_kp = match w_ref.keypair() {
                Ok(kp) => kp,
                Err(_) => return serde_json::json!({"error": "Invalid wallet keypair"}),
            };
            let needed = fee;
            if needed > w_ref.balance {
                return serde_json::json!({"error": "Insufficient balance"});
            }
            let utxos = w_ref.get_utxos_for_amount(needed);
            drop(w);
            let tx = Transaction::contract_call(&sender_kp, contract_addr, function, args, fee, &utxos);
            let tx_hash = tx.hash();
            let mut w = wallet.write().await;
            if let Some(ref mut wallet) = *w {
                for (outpoint, _) in &utxos {
                    let key = format!("{}:{}", hex::encode(outpoint.tx_hash), outpoint.index);
                    wallet.utxos.remove(&key);
                }
                wallet.transactions.push(tx_hash);
                wallet.balance = wallet.utxos.values().map(|(_, a)| a).sum();
            }
            drop(w);
            if let Some(ref st) = storage {
                let w = wallet.read().await;
                if let Some(ref wlt) = *w {
                    if let Ok(s) = st.lock() {
                        let _ = s.save_wallet(wlt);
                        let _ = s.save_transaction(&tx);
                        let _ = s.flush();
                    }
                }
            }
            p2p.broadcast_transaction(&tx).await;
            let _ = p2p.add_to_mempool(tx.clone()).await;
            serde_json::json!({
                "tx_hash": hex::encode(tx_hash),
                "contract_address": addr_hex,
                "function": function,
            })
        }
        "getcontractstate" => {
            let addr_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let contract_addr = match hex::decode(addr_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut a = [0u8; 32];
                    a.copy_from_slice(&b);
                    a
                }
                _ => return serde_json::json!({"error": "Invalid contract address"}),
            };
            let state = match storage {
                Some(ref st) => {
                    if let Ok(s) = st.lock() {
                        if let Ok(Some(data)) = s.load_contract_state(&contract_addr, ":all") {
                            if let Ok(stored) = serde_json::from_slice::<std::collections::HashMap<String, String>>(&data) {
                                stored
                            } else {
                                std::collections::HashMap::new()
                            }
                        } else {
                            std::collections::HashMap::new()
                        }
                    } else {
                        std::collections::HashMap::new()
                    }
                }
                None => std::collections::HashMap::new(),
            };
            serde_json::json!({
                "contract_address": addr_hex,
                "state": state,
            })
        }
        "callcontractview" => {
            let addr_hex = params.get(0).and_then(|p| p.as_str()).unwrap_or("");
            let function = params.get(1).and_then(|p| p.as_str()).unwrap_or("");
            let args_hex = params.get(2).and_then(|p| p.as_str()).unwrap_or("");
            let contract_addr = match hex::decode(addr_hex) {
                Ok(b) if b.len() == 32 => {
                    let mut a = [0u8; 32];
                    a.copy_from_slice(&b);
                    a
                }
                _ => return serde_json::json!({"error": "Invalid contract address"}),
            };
            let args = match hex::decode(args_hex) {
                Ok(a) => a,
                Err(_) => return serde_json::json!({"error": "Invalid args hex"}),
            };
            let code = match storage {
                Some(ref st) => {
                    if let Ok(s) = st.lock() {
                        s.load_contract_code(&contract_addr).ok().flatten()
                    } else { None }
                }
                None => None,
            };
            let code = match code {
                Some(c) => c,
                None => return serde_json::json!({"error": "Contract code not found"}),
            };
            let mut persist = std::collections::HashMap::new();
            if let Some(ref st) = storage {
                if let Ok(s) = st.lock() {
                    if let Ok(Some(data)) = s.load_contract_state(&contract_addr, ":all") {
                        if let Ok(stored) = serde_json::from_slice::<std::collections::HashMap<String, String>>(&data) {
                            for (k, v) in stored {
                                persist.insert(k, v.into_bytes());
                            }
                        }
                    }
                }
            }
            let fn_args = {
                let mut b = function.as_bytes().to_vec();
                b.push(0);
                b.extend_from_slice(&args);
                b
            };
            let mut ctx = ContractContext {
                caller: [0u8; 32],
                block_height: blockchain.read().await.state.height,
                contract_address: contract_addr,
                events: Vec::new(),
                persist,
            };
            let runtime = match WasmRuntime::new() {
                Ok(r) => r,
                Err(_) => return serde_json::json!({"error": "WASM runtime init failed"}),
            };
            match runtime.call(&code, &fn_args, &mut ctx) {
                Ok((gas_used, result)) => {
                    serde_json::json!({
                        "gas_used": gas_used,
                        "result": hex::encode(result),
                        "events": ctx.events.iter().map(|e| hex::encode(e)).collect::<Vec<_>>(),
                    })
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "getblocks" => {
            let start = params.get(0).and_then(|p| p.as_u64()).unwrap_or(0);
            let count = params.get(1).and_then(|p| p.as_u64()).unwrap_or(20).min(100);
            let bc = blockchain.read().await;
            let end = (start + count).min(bc.state.height + 1);
            let mut blocks = Vec::new();
            for h in start..end {
                if let Some(block) = bc.get_block(h) {
                    let reward = block.transactions.first()
                        .map(|tx| tx.outputs.iter().map(|o| o.amount).sum::<u64>())
                        .unwrap_or(0);
                    blocks.push(serde_json::json!({
                        "height": h,
                        "hash": hex::encode(block.hash()),
                        "timestamp": block.header.timestamp,
                        "tx_count": block.transactions.len(),
                        "reward": reward,
                    }));
                }
            }
            serde_json::json!({"blocks": blocks, "start": start, "count": blocks.len(), "total": bc.state.height + 1})
        }
        "getmempoolinfo" => {
            let mempool = p2p.mempool.read().await;
            let txs: Vec<String> = mempool.iter().map(|tx| hex::encode(tx.hash())).collect();
            serde_json::json!({"size": mempool.len(), "txs": txs})
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
