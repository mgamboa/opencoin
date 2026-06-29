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
        mine: bool,
        #[arg(long)]
        premine_key: Option<String>,
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
        Commands::Start { data_dir, p2p_port, rpc_port, mine, premine_key } => {
            run_node(&data_dir, p2p_port, rpc_port, mine, premine_key).await?;
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
    mine: bool,
    premine_key: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let expanded_dir = shellexpand::tilde(data_dir).to_string();
    std::fs::create_dir_all(&expanded_dir)?;
    info!("OpenCoin Node starting...");
    info!("Data directory: {}", expanded_dir);

    let premine_kp = match premine_key {
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

    let blockchain = Arc::new(RwLock::new(Blockchain::new(premine_stealth.clone())));
    {
        let mut bc = blockchain.write().await;
        bc.state.premine_remaining = config::PREMINE_AMOUNT;
    }

    let wallet = Arc::new(RwLock::new(Some(Wallet::from_keypair(premine_kp, "premine"))));
    let p2p = Arc::new(P2PNetwork::new(p2p_port));
    let rpc_server = RpcServer::new(blockchain.clone(), wallet.clone(), p2p.clone(), rpc_port);

    let _storage = if let Ok(s) = Storage::new(&format!("{}/db", expanded_dir)) {
        info!("Storage initialized");
        Some(s)
    } else {
        warn!("Storage initialization failed, running without persistence");
        None
    };

    let bc_for_mining = blockchain.clone();
    let w_for_mining = wallet.clone();
    let p2p_for_mining = p2p.clone();

    if mine {
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

    if let Err(e) = rpc_server.start().await {
        error!("RPC server error: {}", e);
    }

    Ok(())
}

fn generate_genesis(premine_address: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pub_bytes = hex::decode(premine_address)?;
    let public_key = PublicKey::from_bytes(&pub_bytes)?;
    let stealth = StealthAddress {
        spend_pub: public_key.clone(),
        view_pub: public_key,
    };
    let coinbase = Transaction::coinbase(config::PREMINE_AMOUNT, &stealth);
    let genesis = Block::genesis(coinbase);

    let json = serde_json::to_string_pretty(&genesis)?;
    std::fs::write(output, json)?;
    info!("Genesis block written to {}", output);
    Ok(())
}
