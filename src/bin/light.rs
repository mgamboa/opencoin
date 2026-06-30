use clap::Parser;
use std::net::SocketAddr;
use std::str::FromStr;
use log::info;

use opencoin::light_client::LightClient;
use opencoin::crypto::bloom::BloomFilter;
use opencoin::crypto::keys::PublicKey;

#[derive(Parser)]
#[command(name = "opencoin-light")]
#[command(about = "OpenCoin SPV light client")]
struct Cli {
    #[arg(short, long)]
    connect: String,

    #[arg(short, long)]
    watch: Option<String>,

    #[arg(short, long, default_value = "0")]
    from_height: u64,

    #[arg(short, long, default_value = "100")]
    to_height: u64,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let addr = SocketAddr::from_str(&cli.connect).expect("Invalid address");

    let mut client = LightClient::new();
    client.connect(addr).await.expect("Failed to connect");

    info!("Connected to {}", cli.connect);

    client.sync_headers(cli.from_height).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    if let Some(watch_str) = cli.watch {
        let pubkey_bytes = hex::decode(&watch_str).expect("Invalid hex public key");
        let pubkey = PublicKey(pubkey_bytes.try_into().expect("Public key must be 32 bytes"));
        let mut filter = BloomFilter::new(100, 0.01);
        filter.insert_address(&pubkey.0);

        info!("Watching address: {}", watch_str);

        for height in cli.from_height..=cli.to_height {
            client.request_merkle_block(height, filter.clone()).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    info!("Light client done");
}
