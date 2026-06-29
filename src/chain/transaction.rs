use serde::{Deserialize, Serialize};

use crate::crypto::keys::{PublicKey, SignatureBytes};
use crate::crypto::stealth::{KeyImage, OneTimeOutput, StealthAddress};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub key_image: KeyImage,
    pub ring_members: Vec<PublicKey>,
    pub ring_signature: Vec<SignatureBytes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub stealth_address: StealthAddress,
    pub one_time_output: OneTimeOutput,
    pub amount: u64,
    pub view_key_proof: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u32,
    pub tx_type: TransactionType,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub fee: u64,
    pub timestamp: u64,
    pub signatures: Vec<SignatureBytes>,
    pub memo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionType {
    Coinbase,
    Transfer,
    Private,
    SmartContract,
}

impl Transaction {
    pub fn coinbase(reward: u64, recipient: &StealthAddress) -> Self {
        let (one_time_output, _) = crate::crypto::stealth::create_stealth_output(recipient, reward);
        Transaction {
            version: 1,
            tx_type: TransactionType::Coinbase,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                stealth_address: recipient.clone(),
                one_time_output,
                amount: reward,
                view_key_proof: None,
            }],
            fee: 0,
            timestamp: 0,
            signatures: Vec::new(),
            memo: Some(String::from("Coinbase")),
        }
    }

    pub fn transfer(
        sender: &StealthAddress,
        recipient: &StealthAddress,
        amount: u64,
        fee: u64,
        change: u64,
    ) -> Self {
        let (out_recipient, _) = crate::crypto::stealth::create_stealth_output(recipient, amount);
        let mut outputs = vec![
            TxOutput {
                stealth_address: recipient.clone(),
                one_time_output: out_recipient,
                amount,
                view_key_proof: None,
            },
        ];
        if change > 0 {
            let (out_change, _) = crate::crypto::stealth::create_stealth_output(sender, change);
            outputs.push(TxOutput {
                stealth_address: sender.clone(),
                one_time_output: out_change,
                amount: change,
                view_key_proof: None,
            });
        }
        Transaction {
            version: 1,
            tx_type: TransactionType::Transfer,
            inputs: Vec::new(),
            outputs,
            fee,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signatures: Vec::new(),
            memo: Some(format!("Transfer {} OC to {}", 
                amount as f64 / crate::config::COIN as f64,
                hex::encode(&recipient.spend_pub.0[..8]))),
        }
    }

    pub fn coinbase_multi_output(recipients: &[(StealthAddress, u64)]) -> Self {
        let mut outputs = Vec::with_capacity(recipients.len());
        for (addr, amount) in recipients {
            let (one_time_output, _) = crate::crypto::stealth::create_stealth_output(addr, *amount);
            outputs.push(TxOutput {
                stealth_address: addr.clone(),
                one_time_output,
                amount: *amount,
                view_key_proof: None,
            });
        }
        Transaction {
            version: 1,
            tx_type: TransactionType::Coinbase,
            inputs: Vec::new(),
            outputs,
            fee: 0,
            timestamp: 0,
            signatures: Vec::new(),
            memo: Some(String::from("Pool Coinbase")),
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let encoded = serde_json::to_vec(self).unwrap_or_default();
        crate::crypto::hash::double_sha3_256(&encoded)
    }

    pub fn total_output(&self) -> u64 {
        self.outputs.iter().map(|o| o.amount).sum()
    }
}
