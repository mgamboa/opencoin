use std::collections::HashMap;
use serde::{Deserialize, Serialize};


use crate::crypto::keys::{KeyPair, SecretKey, generate_keypair, public_key_to_address};
use crate::crypto::stealth::{StealthAddress, create_stealth_output};
use crate::chain::transaction::{OutPoint, Transaction, TransactionType, TxOutput};
use crate::chain::block::Block;
use crate::chain::address::OpenCoinAddress;

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    #[serde(rename = "keypair")]
    keypair_data: KeypairData,
    pub balance: u64,
    pub locked_balance: u64,
    pub transactions: Vec<[u8; 32]>,
    pub name: String,
    pub utxos: HashMap<String, (OutPoint, u64)>,
}

#[derive(Clone, Serialize, Deserialize)]
struct KeypairData {
    public: Vec<u8>,
    secret: Vec<u8>,
}

impl Wallet {
    pub fn new(name: &str) -> Self {
        let kp = generate_keypair();
        Wallet {
            keypair_data: KeypairData {
                public: kp.public.0.to_vec(),
                secret: kp.secret.0.to_vec(),
            },
            balance: 0,
            locked_balance: 0,
            transactions: Vec::new(),
            name: name.to_string(),
            utxos: HashMap::new(),
        }
    }

    pub fn from_keypair(keypair: KeyPair, name: &str) -> Self {
        Wallet {
            keypair_data: KeypairData {
                public: keypair.public.0.to_vec(),
                secret: keypair.secret.0.to_vec(),
            },
            balance: 0,
            locked_balance: 0,
            transactions: Vec::new(),
            name: name.to_string(),
            utxos: HashMap::new(),
        }
    }

    pub fn keypair(&self) -> Result<KeyPair, &'static str> {
        let sec_key = SecretKey::from_bytes(&self.keypair_data.secret)?;
        Ok(KeyPair::from_secret_key(&sec_key))
    }

    pub fn main_address(&self) -> Result<OpenCoinAddress, &'static str> {
        let kp = self.keypair()?;
        Ok(OpenCoinAddress::new(&kp.public, &kp.public))
    }

    pub fn stealth_address(&self) -> Result<StealthAddress, &'static str> {
        let kp = self.keypair()?;
        Ok(StealthAddress {
            spend_pub: kp.public.clone(),
            view_pub: kp.public.clone(),
        })
    }

    pub fn address_string(&self) -> Result<String, &'static str> {
        let kp = self.keypair()?;
        Ok(public_key_to_address(&kp.public))
    }

    pub fn create_transaction(
        &self,
        recipients: &[(StealthAddress, u64)],
        fee: u64,
    ) -> Result<Transaction, &'static str> {
        let mut outputs = Vec::new();
        let mut total_out = 0u64;

        for (recipient, amount) in recipients {
            let (output, _r) = create_stealth_output(recipient, *amount);
            outputs.push(TxOutput {
                stealth_address: recipient.clone(),
                one_time_output: output,
                amount: *amount,
                view_key_proof: None,
            });
            total_out += amount;
        }

        Ok(Transaction {
            version: 1,
            tx_type: TransactionType::Private,
            inputs: Vec::new(),
            outputs,
            fee,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signatures: Vec::new(),
            memo: None,
        })
    }

    pub fn scan_block(&mut self, block: &Block) -> u64 {
        let my_addr = match self.stealth_address() {
            Ok(a) => a,
            Err(_) => return 0,
        };
        let tx_hash_hex = |tx: &Transaction| hex::encode(tx.hash());

        for tx in &block.transactions {
            let txh = tx_hash_hex(tx);
            if !self.transactions.contains(&tx.hash()) {
                self.transactions.push(tx.hash());
            }

            for (i, output) in tx.outputs.iter().enumerate() {
                if output.stealth_address.spend_pub.0 == my_addr.spend_pub.0 {
                    let outpoint = OutPoint { tx_hash: tx.hash(), index: i as u32 };
                    let key = format!("{}:{}", txh, i);
                    self.utxos.insert(key, (outpoint, output.amount));
                }
            }

            for input in &tx.inputs {
                let in_key = format!("{}:{}", hex::encode(input.outpoint.tx_hash), input.outpoint.index);
                if self.utxos.remove(&in_key).is_some() {
                    // spent one of our UTXOs
                }
            }
        }

        self.balance = self.utxos.values().map(|(_, amt)| amt).sum();
        self.balance
    }

    pub fn get_utxos_for_amount(&self, needed: u64) -> Vec<(OutPoint, u64)> {
        let mut selected = Vec::new();
        let mut total = 0u64;
        for (_, (outpoint, amount)) in &self.utxos {
            selected.push((outpoint.clone(), *amount));
            total += amount;
            if total >= needed {
                break;
            }
        }
        selected
    }

    pub fn to_json(&self) -> String {
        let addr = self.address_string().unwrap_or_default();
        let pub_hex = hex::encode(&self.keypair_data.public);
        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.name,
            "address": addr,
            "balance": self.balance,
            "public_key": pub_hex,
            "sub_addresses": 0,
        }))
        .unwrap_or_default()
    }
}
