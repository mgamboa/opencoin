use serde::{Deserialize, Serialize};

use crate::crypto::keys::{KeyPair, PublicKey, SignatureBytes};
use crate::crypto::stealth::{KeyImage, OneTimeOutput, StealthAddress};
use crate::crypto::ringct::{PedersenCommitment, RangeProof, RingSignature, random_scalar};

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct OutPoint {
    pub tx_hash: [u8; 32],
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub outpoint: OutPoint,
    pub key_image: KeyImage,
    pub signature: SignatureBytes,
    pub pubkey: PublicKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub stealth_address: StealthAddress,
    pub one_time_output: OneTimeOutput,
    pub amount: u64,
    pub commitment: Option<PedersenCommitment>,
    pub range_proof: Option<RangeProof>,
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
    pub ring_signature: Option<RingSignature>,
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
                commitment: None,
                range_proof: None,
                view_key_proof: None,
            }],
            fee: 0,
            timestamp: 0,
            signatures: Vec::new(),
            ring_signature: None,
            memo: Some(String::from("Coinbase")),
        }
    }

    pub fn sign_hash(&self, keypair: &KeyPair, outpoint_index: usize) -> SignatureBytes {
        let mut data = self.hash().to_vec();
        data.extend_from_slice(&(outpoint_index as u32).to_le_bytes());
        let hash = crate::crypto::hash::double_sha3_256(&data);
        keypair.sign(&hash).unwrap()
    }

    pub fn transfer(
        sender_kp: &KeyPair,
        recipient: &StealthAddress,
        amount: u64,
        fee: u64,
        utxos: &[(OutPoint, u64)],
    ) -> Self {
        let total_in: u64 = utxos.iter().map(|(_, amt)| amt).sum();
        let change = total_in - amount - fee;
        let sender_stealth = StealthAddress {
            spend_pub: sender_kp.public.clone(),
            view_pub: sender_kp.public.clone(),
        };

        let (out_recipient, _) = crate::crypto::stealth::create_stealth_output(recipient, amount);
        let mut outputs = vec![
            TxOutput {
                stealth_address: recipient.clone(),
                one_time_output: out_recipient,
                amount,
                commitment: None,
                range_proof: None,
                view_key_proof: None,
            },
        ];
        if change > 0 {
            let (out_change, _) = crate::crypto::stealth::create_stealth_output(&sender_stealth, change);
            outputs.push(TxOutput {
                stealth_address: sender_stealth,
                one_time_output: out_change,
                amount: change,
                commitment: None,
                range_proof: None,
                view_key_proof: None,
            });
        }

        let mut tx = Transaction {
            version: 1,
            tx_type: TransactionType::Transfer,
            inputs: utxos.iter().map(|(outpoint, _)| TxInput {
                outpoint: outpoint.clone(),
                key_image: KeyImage([0u8; 32]),
                signature: SignatureBytes([0u8; 64]),
                pubkey: sender_kp.public.clone(),
            }).collect(),
            outputs,
            fee,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signatures: Vec::new(),
            ring_signature: None,
            memo: Some(format!("Transfer {} OC to {}",
                amount as f64 / crate::config::COIN as f64,
                hex::encode(&recipient.spend_pub.0[..8]))),
        };

        for (i, _) in utxos.iter().enumerate() {
            let sig = tx.sign_hash(sender_kp, i);
            tx.inputs[i].signature = sig;
        }

        tx
    }

    pub fn transfer_private(
        sender_kp: &KeyPair,
        recipient: &StealthAddress,
        amount: u64,
        fee: u64,
        utxos: &[(OutPoint, u64)],
        ring_members: &[crate::crypto::ringct::RingMember],
    ) -> Self {
        let total_in: u64 = utxos.iter().map(|(_, amt)| amt).sum();
        let change = total_in.saturating_sub(amount + fee);
        let sender_stealth = StealthAddress {
            spend_pub: sender_kp.public.clone(),
            view_pub: sender_kp.public.clone(),
        };

        let blinding_recip = random_scalar();
        let (range_recip, _) = RangeProof::prove(amount, &blinding_recip);
        let mut outputs = vec![
            TxOutput {
                stealth_address: recipient.clone(),
                one_time_output: OneTimeOutput {
                    ephemeral_pub: crate::crypto::stealth::EphemeralPublicKey(PublicKey([0u8; 32])),
                    key_image: KeyImage([0u8; 32]),
                    amount_commitment: range_recip.commitment.0,
                },
                amount: 0,
                commitment: Some(range_recip.commitment.clone()),
                range_proof: Some(range_recip),
                view_key_proof: None,
            },
        ];
        let change_blinding = random_scalar();
        if change > 0 {
            let (range_change, _) = RangeProof::prove(change, &change_blinding);
            outputs.push(TxOutput {
                stealth_address: sender_stealth,
                one_time_output: OneTimeOutput {
                    ephemeral_pub: crate::crypto::stealth::EphemeralPublicKey(PublicKey([0u8; 32])),
                    key_image: KeyImage([0u8; 32]),
                    amount_commitment: range_change.commitment.0,
                },
                amount: 0,
                commitment: Some(range_change.commitment.clone()),
                range_proof: Some(range_change),
                view_key_proof: None,
            });
        }

        let mut tx = Transaction {
            version: 2,
            tx_type: TransactionType::Private,
            inputs: utxos.iter().map(|(outpoint, _)| TxInput {
                outpoint: outpoint.clone(),
                key_image: KeyImage(crate::crypto::ringct::generate_key_image(&sender_kp.secret)),
                signature: SignatureBytes([0u8; 64]),
                pubkey: sender_kp.public.clone(),
            }).collect(),
            outputs,
            fee,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signatures: Vec::new(),
            ring_signature: None,
            memo: None,
        };

        let tx_hash = tx.hash();
        let ring_sig = RingSignature::sign(
            &tx_hash,
            0,
            ring_members,
            &sender_kp.secret,
            &sender_kp.public,
        ).unwrap_or(RingSignature::new(ring_members.len() as u32));

        for (i, _) in utxos.iter().enumerate() {
            let sig = tx.sign_hash(sender_kp, i);
            tx.inputs[i].signature = sig;
        }

        tx.ring_signature = Some(ring_sig);
        tx
    }

    pub fn coinbase_multi_output(recipients: &[(StealthAddress, u64)]) -> Self {
        let mut outputs = Vec::with_capacity(recipients.len());
        for (addr, amount) in recipients {
            let (one_time_output, _) = crate::crypto::stealth::create_stealth_output(addr, *amount);
            outputs.push(TxOutput {
                stealth_address: addr.clone(),
                one_time_output,
                amount: *amount,
                commitment: None,
                range_proof: None,
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
            ring_signature: None,
            memo: Some(String::from("Pool Coinbase")),
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let encoded = serde_json::to_vec(self).unwrap_or_default();
        crate::crypto::hash::double_sha3_256(&encoded)
    }

    pub fn total_output(&self) -> u64 {
        if self.tx_type == TransactionType::Private {
            return self.fee;
        }
        self.outputs.iter().map(|o| o.amount).sum()
    }

    pub fn total_input(&self) -> u64 {
        self.inputs.iter().map(|_| 0).sum()
    }

    pub fn verify_signatures(&self) -> Result<(), &'static str> {
        if self.tx_type == TransactionType::Private {
            if let Some(ref ring_sig) = self.ring_signature {
                let tx_hash = self.hash();
                ring_sig.verify(&tx_hash)?;
                return Ok(());
            }
            return Err("Private tx missing ring signature");
        }
        for (i, input) in self.inputs.iter().enumerate() {
            let mut data = self.hash().to_vec();
            data.extend_from_slice(&(i as u32).to_le_bytes());
            let hash = crate::crypto::hash::double_sha3_256(&data);
            input.signature.verify(&hash, &input.pubkey)?;
        }
        Ok(())
    }
}
