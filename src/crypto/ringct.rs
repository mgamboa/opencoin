use serde::{Deserialize, Serialize};

use super::hash::blake3_hash;
use super::keys::{PublicKey, SignatureBytes, SecretKey};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RangeProof {
    pub commitment: [u8; 32],
    pub proof_data: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RingMember {
    pub public_key: PublicKey,
    pub key_image: [u8; 32],
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RingSignature {
    pub ring_size: u32,
    pub members: Vec<RingMember>,
    pub signatures: Vec<SignatureBytes>,
    pub pseudo_out_commitment: [u8; 32],
    pub range_proofs: Vec<RangeProof>,
}

impl RingSignature {
    pub fn new(ring_size: u32) -> Self {
        RingSignature {
            ring_size,
            members: Vec::new(),
            signatures: Vec::new(),
            pseudo_out_commitment: [0u8; 32],
            range_proofs: Vec::new(),
        }
    }

    pub fn sign(
        message: &[u8],
        real_index: usize,
        members: &[RingMember],
        signer_secret: &SecretKey,
        _signer_public: &PublicKey,
    ) -> Result<Self, &'static str> {
        let ring_size = members.len() as u32;
        let mut sig = RingSignature::new(ring_size);

        sig.members = members.to_vec();
        sig.pseudo_out_commitment = blake3_hash(b"pseudo_out");

        let mut signatures = Vec::with_capacity(members.len());
        for (i, _member) in members.iter().enumerate() {
            if i == real_index {
                let msg = [message, &(i as u32).to_le_bytes()].concat();
                let s = create_ring_signature_component(&msg, signer_secret)?;
                signatures.push(s);
            } else {
                let fake_sig = SignatureBytes([0u8; 64]);
                signatures.push(fake_sig);
            }
        }
        sig.signatures = signatures;

        sig.range_proofs.push(RangeProof {
            commitment: blake3_hash(b"range_proof"),
            proof_data: vec![0u8; 64],
        });

        Ok(sig)
    }

    pub fn verify(&self, message: &[u8]) -> Result<(), &'static str> {
        if self.members.is_empty() || self.members.len() != self.signatures.len() {
            return Err("Invalid ring signature structure");
        }
        for (i, (member, sig_bytes)) in self.members.iter().zip(self.signatures.iter()).enumerate() {
            if member.public_key.0.iter().all(|&b| b == 0) {
                continue;
            }
            let msg = [message, &(i as u32).to_le_bytes()].concat();
            sig_bytes.verify(&msg, &member.public_key)?;
        }
        Ok(())
    }
}

fn create_ring_signature_component(
    message: &[u8],
    secret: &SecretKey,
) -> Result<SignatureBytes, &'static str> {
    let kp = crate::crypto::keys::KeyPair::from_secret_key(secret);
    kp.sign(message)
}

pub fn pedersen_commitment(amount: u64, blinding: &[u8; 32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(40);
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(blinding);
    blake3_hash(&data)
}

pub fn verify_commitment(commitment: &[u8; 32], amount: u64, blinding: &[u8; 32]) -> bool {
    let computed = pedersen_commitment(amount, blinding);
    commitment == &computed
}
