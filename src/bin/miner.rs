use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "opencoin-miner")]
#[command(about = "OpenCoin CPU miner (connects to pool)")]
struct Cli {
    #[arg(short, long, default_value = "127.0.0.1:3333")]
    pool: String,
    #[arg(short, long, default_value_t = 1)]
    threads: u32,
    #[arg(short, long)]
    address: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    println!("OpenCoin Miner starting...");
    println!("Pool: {}", cli.pool);
    println!("Threads: {}", cli.threads);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        println!("\nMiner stopping...");
    })?;

    let stream = TcpStream::connect(&cli.pool).await?;
    println!("Connected to pool");

    let (read_half, mut write_half) = tokio::io::split(stream);

    let (submit_tx, mut submit_rx) = mpsc::unbounded_channel::<String>();

    let write_task = tokio::spawn(async move {
        while let Some(msg) = submit_rx.recv().await {
            let mut line = msg;
            line.push('\n');
            if write_half.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let current_job = Arc::new(tokio::sync::RwLock::new(None::<PoolJob>));

    let reader_job = current_job.clone();
    let reader_task = tokio::spawn(async move {
        let mut lines = BufReader::new(read_half).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() { continue; }
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                match msg["type"].as_str().unwrap_or("") {
                    "job" => {
                        let job_id = msg["job_id"].as_u64().unwrap_or(0);
                        let height = msg["height"].as_u64().unwrap_or(0);
                        let block_target = msg["target"].as_u64().unwrap_or(0);
                        let share_target = msg["share_target"].as_u64().unwrap_or(0);
                        let header_hex = msg["header"].as_str().unwrap_or("");
                        let header_bytes = hex::decode(header_hex).unwrap_or_default();

                        let job = PoolJob {
                            job_id,
                            height,
                            block_target,
                            share_target,
                            header_bytes,
                        };

                        let mut j = reader_job.write().await;
                        *j = Some(job);
                        println!("New job #{} at height {}", job_id, height);
                    }
                    "result" => {
                        let status = msg["status"].as_str().unwrap_or("");
                        match status {
                            "accepted" => print!("."),
                            _ => print!("x"),
                        }
                        use std::io::{Write, stdout};
                        stdout().flush().ok();
                    }
                    _ => {}
                }
            }
        }
    });

    let miner_address = cli.address.clone();

    for thread_id in 0..cli.threads {
        let job_ref = current_job.clone();
        let submit_tx = submit_tx.clone();
        let run = running.clone();
        let addr = miner_address.clone();

        tokio::spawn(async move {
            let mut nonce = thread_id as u64 * 1_000_000_000;
            let mut last_report = SystemTime::now();
            let mut hashrate_samples = 0u64;

            while run.load(Ordering::SeqCst) {
                let job = {
                    let j = job_ref.read().await;
                    j.clone()
                };

                if let Some(ref job) = job {
                    let mut full_header = job.header_bytes.clone();
                    full_header.extend_from_slice(&nonce.to_le_bytes());
                    full_header.extend_from_slice(&0u64.to_le_bytes());

                    let hash = opencoin::crypto::hash::double_sha3_256(&full_header);
                    let hash_val = u64::from_le_bytes(hash[24..32].try_into().unwrap_or([0u8; 8]));

                    if hash_val <= job.share_target {
                        let submit = format!(
                            r#"{{"type":"submit","job_id":{},"nonce":{},"thread":{},"address":"{}"}}"#,
                            job.job_id, nonce, thread_id, addr
                        );
                        let _ = submit_tx.send(submit);
                    }

                    nonce += 1;
                    hashrate_samples += 1;

                    if hashrate_samples >= 100_000 {
                        let elapsed = last_report.elapsed().unwrap_or(Duration::from_secs(1));
                        let khs = (hashrate_samples as f64 / elapsed.as_secs_f64()) / 1000.0;
                        if thread_id == 0 {
                            print!("\rThread {}: {:.0} KH/s, nonce: {}", thread_id, khs, nonce);
                        }
                        use std::io::{Write, stdout};
                        stdout().flush().ok();
                        hashrate_samples = 0;
                        last_report = SystemTime::now();
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
    }

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if !running.load(Ordering::SeqCst) {
            break;
        }
    }

    drop(submit_tx);
    let _ = write_task.await;

    println!("\nMiner stopped.");
    Ok(())
}

#[derive(Clone)]
struct PoolJob {
    job_id: u64,
    height: u64,
    block_target: u64,
    share_target: u64,
    header_bytes: Vec<u8>,
}
