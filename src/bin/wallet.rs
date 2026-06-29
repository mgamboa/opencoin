use clap::{Parser, Subcommand};
use opencoin::crypto::keys::{KeyPair, SecretKey, PublicKey, generate_keypair};
use opencoin::chain::address::OpenCoinAddress;
use opencoin::wallet::Wallet;

#[derive(Parser)]
#[command(name = "opencoin-wallet")]
#[command(about = "OpenCoin wallet CLI")]
struct Cli {
    #[command(subcommand)]
    command: WalletCommands,
}

#[derive(Subcommand)]
enum WalletCommands {
    Create {
        #[arg(short, long, default_value = "my-wallet")]
        name: String,
        #[arg(short, long, default_value = "~/.opencoin/wallets")]
        dir: String,
    },
    Show {
        #[arg(short, long, default_value = "my-wallet")]
        name: String,
        #[arg(short, long, default_value = "~/.opencoin/wallets")]
        dir: String,
    },
    GenerateKey {
        #[arg(short, long)]
        seed: Option<String>,
    },
    Validate {
        address: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        WalletCommands::Create { name, dir } => {
            let expanded_dir = shellexpand::tilde(&dir).to_string();
            std::fs::create_dir_all(&expanded_dir)?;
            let wallet = Wallet::new(&name);
            let wallet_path = format!("{}/{}.json", expanded_dir, name);
            let json = wallet.to_json();
            std::fs::write(&wallet_path, &json)?;
            println!("Wallet created: {}", wallet_path);
            println!("Address: {}", wallet.address_string().unwrap_or_default());
            let kp = wallet.keypair().unwrap();
            println!("Public key: {}", hex::encode(kp.public.0));
            println!("Secret key: {}", hex::encode(kp.secret.0));
            println!("SAVE YOUR SECRET KEY!");
        }
        WalletCommands::Show { name, dir } => {
            let expanded_dir = shellexpand::tilde(&dir).to_string();
            let wallet_path = format!("{}/{}.json", expanded_dir, name);
            let json = std::fs::read_to_string(&wallet_path)?;
            println!("{}", json);
        }
        WalletCommands::GenerateKey { seed: _seed } => {
            let kp = generate_keypair();
            println!("Public key:  {}", hex::encode(kp.public.0));
            println!("Secret key:  {}", hex::encode(kp.secret.0));
            println!("Address:     {}", opencoin::crypto::keys::public_key_to_address(&kp.public));
        }
        WalletCommands::Validate { address } => {
            match OpenCoinAddress::from_string(&address) {
                Ok(addr) => println!("Valid address: {}", addr.to_string()),
                Err(e) => println!("Invalid address: {}", e),
            }
        }
    }
    Ok(())
}
