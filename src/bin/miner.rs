use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use opencoin::config;
use opencoin::chain::block::Block;
use opencoin::chain::transaction::{Transaction, TransactionType};
use opencoin::consensus::pow::{mine_block, calculate_target};
use opencoin::consensus::difficulty::{calculate_difficulty, difficulty_to_target};
use opencoin::crypto::hash::merkle_root;
use opencoin::crypto::keys::{PublicKey, SecretKey, KeyPair};
use opencoin::crypto::stealth::StealthAddress;
use opencoin::chain::block::BlockHeader;

#[derive(Parser)]
#[command(name = "opencoin-miner")]
#[command(about = "OpenCoin CPU miner")]
struct Cli {
    #[arg(short, long)]
    address: String,
    #[arg(short, long, default_value_t = 1)]
    threads: u32,
    #[arg(short, long, default_value_t = config::RPC_PORT)]
    rpc_port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    println!("OpenCoin Miner starting...");
    println!("Mining to address: {}", cli.address);
    println!("Threads: {}", cli.threads);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        println!("Miner stopping...");
    })?;

    let mut handles = vec![];
    for thread_id in 0..cli.threads {
        let addr = cli.address.clone();
        let run = running.clone();
        let handle = thread::spawn(move || {
            let pub_bytes = hex::decode(&addr).unwrap_or_default();
            let public_key = PublicKey::from_bytes(&pub_bytes).ok();
            let stealth = public_key.map(|pk| StealthAddress {
                spend_pub: pk.clone(),
                view_pub: pk,
            });

            let mut height: u64 = 0;
            while run.load(Ordering::SeqCst) {
                height += 1;
                if thread_id == 0 {
                    print!("\rMining height {}...", height);
                    use std::io::{Write, stdout};
                    stdout().flush().ok();
                }
                thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().ok();
    }

    println!("\nMiner stopped.");
    Ok(())
}
