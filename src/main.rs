use clap::{Parser, Subcommand, Command};
use log::{info, warn, error};
use std::sync::Arc;
use tokio::sync::RwLock;

use opencoin::config;
use opencoin::chain::blockchain::Blockchain;
use opencoin::chain::block::Block;
use opencoin::chain::transaction::{Transaction, TransactionType};
use opencoin::consensus::pow::{mine_block, calculate_target};
use opencoin::consensus::difficulty::{calculate_difficulty, difficulty_to_compact};
use opencoin::crypto::hash::merkle_root;
use opencoin::crypto::stealth::StealthAddress;
use opencoin::crypto::keys::{PublicKey, KeyPair, SecretKey};
use opencoin::p2p::P2PNetwork;
use opencoin::rpc::RpcServer;
use opencoin::pool::PoolServer;
use opencoin::storage::db::Storage;
use opencoin::wallet::Wallet;

#[derive(Parser)]
#[command(name = "opencoin-node")]
#[command(about = "OpenCoin blockchain node")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start {
        #[arg(short, long, default_value = "~/.opencoin")]
        data_dir: String,
        #[arg(short, long, default_value_t = config::P2P_PORT)]
        p2p_port: u16,
        #[arg(short, long, default_value_t = config::RPC_PORT)]
        rpc_port: u16,
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        seed: Vec<String>,
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        premine_key: Option<String>,
        #[arg(long)]
        pool: bool,
        #[arg(long, default_value_t = 3333)]
        pool_port: u16,
        #[arg(long)]
        pool_address: Option<String>,
    },
    GenerateGenesis {
        #[arg(long)]
        premine_address: String,
        #[arg(long, default_value = "genesis.json")]
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { data_dir, p2p_port, rpc_port, peer, seed, mine, premine_key, pool, pool_port, pool_address } => {
            run_node(&data_dir, p2p_port, rpc_port, peer.as_deref(), &seed, mine, premine_key, pool, pool_port, pool_address).await?;
        }
        Commands::GenerateGenesis { premine_address, output } => {
            generate_genesis(&premine_address, &output)?;
        }
    }

    Ok(())
}

async fn run_node(
    data_dir: &str,
    p2p_port: u16,
    rpc_port: u16,
    peer: Option<&str>,
    seeds: &[String],
    enable_mining: bool,
    premine_key_hex: Option<String>,
    enable_pool: bool,
    pool_port: u16,
    pool_address: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let expanded_dir = shellexpand::tilde(data_dir).to_string();
    std::fs::create_dir_all(&expanded_dir)?;
    info!("OpenCoin Node starting...");
    info!("Data directory: {}", expanded_dir);

    let has_premine_key = premine_key_hex.is_some();
    let premine_kp = match premine_key_hex {
        Some(key_hex) => {
            let key_bytes = hex::decode(&key_hex)?;
            let secret = SecretKey::from_bytes(&key_bytes)?;
            KeyPair::from_secret_key(&secret)
        }
        None => {
            let kp = KeyPair::generate();
            info!("Generated new premine keypair");
            info!("Premine public key: {}", hex::encode(kp.public.0));
            info!("Premine secret key: {}", hex::encode(kp.secret.0));
            info!("SAVE THIS SECRET KEY!");
            kp
        }
    };

    let premine_stealth = StealthAddress {
        spend_pub: premine_kp.public.clone(),
        view_pub: premine_kp.public.clone(),
    };

    let storage_path = format!("{}/db", expanded_dir);
    let storage = match Storage::new(&storage_path) {
        Ok(s) => {
            info!("Storage initialized at {}", storage_path);
            Some(Arc::new(std::sync::Mutex::new(s)))
        }
        Err(e) => {
            warn!("Storage initialization failed ({}), running without persistence", e);
            None
        }
    };

    let blockchain = if let Some(ref storage) = storage {
        let s = storage.lock().unwrap();
        match Blockchain::load_from_storage(&s, premine_stealth.clone())? {
            Some(bc) => {
                info!("Loaded blockchain from storage: height={}, blocks={}", bc.state.height, bc.blocks.len());
                bc
            }
            None => {
                info!("No existing blockchain found, starting fresh");
                let mut bc = Blockchain::new(premine_stealth.clone());
                bc.state.premine_remaining = config::PREMINE_AMOUNT;
                bc
            }
        }
    } else {
        let mut bc = Blockchain::new(premine_stealth.clone());
        bc.state.premine_remaining = config::PREMINE_AMOUNT;
        bc
    };

    let blockchain = Arc::new(RwLock::new(blockchain));
    if let Some(ref storage) = storage {
        let mut bc = blockchain.write().await;
        bc.set_storage(storage.clone());
    }

    let wallet = Arc::new(RwLock::new({
        if has_premine_key {
            if let Some(ref storage) = storage {
                let s = storage.lock().unwrap();
                if let Ok(Some(saved_wallet)) = s.load_wallet() {
                    info!("Loaded wallet from storage: balance={}, txs={}", saved_wallet.balance, saved_wallet.transactions.len());
                    Some(saved_wallet)
                } else {
                    let mut w = Wallet::from_keypair(premine_kp, "premine");
                    w.transactions.push([0u8; 32]);
                    use opencoin::chain::transaction::OutPoint;
                    let premine_outpoint = OutPoint { tx_hash: [0u8; 32], index: 0 };
                    w.utxos.insert("premine".to_string(), (premine_outpoint, config::PREMINE_AMOUNT));
                    w.balance = config::PREMINE_AMOUNT;
                    let _ = s.save_wallet(&w);
                    Some(w)
                }
            } else {
                let mut w = Wallet::from_keypair(premine_kp, "premine");
                w.transactions.push([0u8; 32]);
                use opencoin::chain::transaction::OutPoint;
                let premine_outpoint = OutPoint { tx_hash: [0u8; 32], index: 0 };
                w.utxos.insert("premine".to_string(), (premine_outpoint, config::PREMINE_AMOUNT));
                w.balance = config::PREMINE_AMOUNT;
                Some(w)
            }
        } else {
            None
        }
    }));
    let p2p = {
        let mut p = P2PNetwork::new(p2p_port, blockchain.clone()).with_wallet(wallet.clone());
        if let Some(ref storage) = storage {
            p = p.with_storage(storage.clone());
        }
        Arc::new(p)
    };

    let pool_server: Option<Arc<PoolServer>> = if enable_pool {
        let pool_addr_stealth = match pool_address {
            Some(ref addr_hex) => {
                let pub_bytes = hex::decode(addr_hex)?;
                let public_key = opencoin::crypto::keys::PublicKey::from_bytes(&pub_bytes)?;
                StealthAddress {
                    spend_pub: public_key.clone(),
                    view_pub: public_key,
                }
            }
            None => {
                let kp = opencoin::crypto::keys::KeyPair::generate();
                info!("Generated pool wallet keypair");
                info!("Pool public key: {}", hex::encode(kp.public.0));
                info!("Pool secret key: {}", hex::encode(kp.secret.0));
                info!("SAVE THIS POOL SECRET KEY!");
                StealthAddress {
                    spend_pub: kp.public.clone(),
                    view_pub: kp.public,
                }
            }
        };
        let mut pool = PoolServer::new(pool_port, pool_addr_stealth, blockchain.clone()).with_p2p(p2p.clone()).with_wallet(wallet.clone());
        if let Some(ref storage) = storage {
            pool = pool.with_storage(storage.clone());
        }
        let pool_arc = Arc::new(pool);
        info!("Pool server configured on port {}", pool_port);
        Some(pool_arc)
    } else {
        None
    };

    let mut rpc_server = RpcServer::new(blockchain.clone(), wallet.clone(), p2p.clone(), pool_server.clone(), rpc_port);
    if let Some(ref storage) = storage {
        rpc_server = rpc_server.with_storage(storage.clone());
    }

    let seed_list: Vec<String> = {
        let mut s: Vec<String> = seeds.to_vec();
        if let Some(peer_addr) = peer {
            s.push(peer_addr.to_string());
        }
        s
    };
    let p2p_clone = p2p.clone();
    tokio::spawn(async move {
        loop {
            for seed_str in &seed_list {
                info!("Connecting to seed peer: {}", seed_str);
                match tokio::net::lookup_host(seed_str).await {
                    Ok(mut addrs) => {
                        if let Some(addr) = addrs.next() {
                            if let Err(e) = p2p_clone.connect_to_peer(addr).await {
                                warn!("Failed to connect to seed {}: {}", seed_str, e);
                            } else {
                                p2p_clone.send_to(&addr, &opencoin::p2p::Message::GetBlocks(0)).await;
                                p2p_clone.send_to(&addr, &opencoin::p2p::Message::GetPeers).await;
                                info!("Connected to seed {}, requesting blocks & peers", seed_str);
                            }
                        } else {
                            warn!("Could not resolve seed: {}", seed_str);
                        }
                    }
                    Err(e) => warn!("Failed to resolve seed {}: {}", seed_str, e),
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    });

    let bc_for_mining = blockchain.clone();
    let w_for_mining = wallet.clone();
    let p2p_for_mining = p2p.clone();

    if enable_mining {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                let mut bc = bc_for_mining.write().await;
                let height = bc.state.height + 1;
                let reward = opencoin::chain::block::calculate_block_reward(height, bc.state.premine_remaining);

                let w = w_for_mining.read().await;
                let wallet = match w.as_ref() {
                    Some(w) => w,
                    None => continue,
                };
                let recipient = wallet.stealth_address().unwrap();
                let coinbase_tx = Transaction::coinbase(reward, &recipient);
                let txs = vec![coinbase_tx];
                let tx_hashes: Vec<[u8; 32]> = txs.iter().map(|t| t.hash()).collect();
                let merkle = merkle_root(&tx_hashes);

                let difficulty = calculate_difficulty(&bc.blocks);
                let compact_target = difficulty_to_compact(difficulty);

                let mut block = Block {
                    header: opencoin::chain::block::BlockHeader {
                        version: 1,
                        height,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        previous_hash: bc.state.current_hash,
                        merkle_root: merkle,
                        difficulty_target: compact_target,
                        nonce: 0,
                        extra_nonce: 0,
                    },
                    transactions: txs,
                };

                let pow_target = calculate_target(difficulty);
                info!("Mining block {} with difficulty {} (target: {})", height, difficulty, pow_target);

                if let Some(nonce) = mine_block(&mut block, pow_target, 1_000_000) {
                    info!("Block {} mined! Nonce: {}", height, nonce);
                    if let Err(e) = bc.add_block(block.clone()) {
                        error!("Failed to add mined block: {}", e);
                    } else {
                        let _ = p2p_for_mining.broadcast_block(&block).await;
                        info!("Height now: {}", bc.state.height);
                    }
                }
            }
        });
    }

    let p2p_clone = p2p.clone();
    tokio::spawn(async move {
        if let Err(e) = p2p_clone.start().await {
            error!("P2P server error: {}", e);
        }
    });

    if let Some(ref pool) = pool_server {
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = pool_clone.start().await {
                error!("Pool server error: {}", e);
            }
        });
    }

    if let Err(e) = rpc_server.start().await {
        error!("RPC server error: {}", e);
    }

    Ok(())
}

fn generate_genesis(_premine_address: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let genesis = Block::genesis();
    let json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(output, json)?;
    info!("Genesis block written to {}", output);
    Ok(())
}
