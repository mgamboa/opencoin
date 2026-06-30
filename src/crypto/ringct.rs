use std::sync::OnceLock;
use curve25519_dalek::{
    scalar::Scalar,
    edwards::{CompressedEdwardsY, EdwardsPoint},
    traits::Identity,
};
use serde::{Deserialize, Serialize};
use super::hash::blake3_hash;
use super::keys::{PublicKey, SignatureBytes, SecretKey};

fn hash_to_point(seed: &[u8]) -> EdwardsPoint {
    let mut hash = blake3_hash(seed);
    loop {
        let compressed = CompressedEdwardsY(hash);
        if let Some(point) = compressed.decompress() {
            if point.is_torsion_free() {
                return point;
            }
        }
        hash = blake3_hash(&hash);
    }
}

fn h_generator() -> &'static EdwardsPoint {
    static H: OnceLock<EdwardsPoint> = OnceLock::new();
    H.get_or_init(|| hash_to_point(b"OpenCoin RingCT H Generator"))
}

fn h_key_image() -> &'static EdwardsPoint {
    static H: OnceLock<EdwardsPoint> = OnceLock::new();
    H.get_or_init(|| hash_to_point(b"OpenCoin RingCT Key Image"))
}

fn scalar_from_hash(hash: &[u8; 32]) -> Scalar {
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(hash);
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PedersenCommitment(pub [u8; 32]);

impl PedersenCommitment {
    pub fn new(amount: u64, blinding: &Scalar) -> Self {
        let b_g = EdwardsPoint::mul_base(blinding);
        let h_times = *h_generator() * Scalar::from(amount);
        let commit = b_g + h_times;
        PedersenCommitment(commit.compress().to_bytes())
    }

    pub fn zero(blinding: &Scalar) -> Self {
        let b_g = EdwardsPoint::mul_base(blinding);
        PedersenCommitment(b_g.compress().to_bytes())
    }

    pub fn to_point(&self) -> Option<EdwardsPoint> {
        CompressedEdwardsY(self.0).decompress()
    }

    pub fn verify(&self, amount: u64, blinding: &Scalar) -> bool {
        let expected = Self::new(amount, blinding);
        self.0 == expected.0
    }
}

pub fn random_scalar() -> Scalar {
    let mut bytes = [0u8; 64];
    getrandom::fill(&mut bytes).expect("RNG failed");
    Scalar::from_bytes_mod_order_wide(&bytes)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RangeProof {
    pub commitment: PedersenCommitment,
    pub bit_commitments: Vec<PedersenCommitment>,
    pub bit_proofs: Vec<SignatureBytes>,
}

impl RangeProof {
    pub fn prove(amount: u64, total_blinding: &Scalar) -> (Self, Vec<Scalar>) {
        let commitment = PedersenCommitment::new(amount, total_blinding);
        let mut bit_commitments = Vec::with_capacity(64);
        let mut bit_proofs = Vec::with_capacity(64);
        let mut bit_blindings = Vec::with_capacity(64);

        let message = commitment.0;

        for i in 0..64 {
            let bit = (amount >> i) & 1;
            let blinding = random_scalar();
            let bit_comm = if bit == 1 {
                PedersenCommitment::new(bit, &blinding)
            } else {
                PedersenCommitment::zero(&blinding)
            };

            let proof_msg = [&message[..], &(i as u64).to_le_bytes()[..], &bit_comm.0[..]].concat();
            let proof_hash = blake3_hash(&proof_msg);
            let sig = sign_with_scalar(&proof_hash, &blinding);

            bit_commitments.push(bit_comm);
            bit_proofs.push(SignatureBytes(sig));
            bit_blindings.push(blinding);
        }

        (RangeProof { commitment, bit_commitments, bit_proofs }, bit_blindings)
    }

    pub fn verify(&self) -> bool {
        let message = self.commitment.0;
        let mut sum_point = EdwardsPoint::identity();

        for (i, (bit_comm, bit_proof)) in self.bit_commitments.iter().zip(self.bit_proofs.iter()).enumerate() {
            let comm_point = match bit_comm.to_point() {
                Some(p) => p,
                None => return false,
            };
            sum_point = sum_point + comm_point;

            let proof_msg = [&message[..], &(i as u64).to_le_bytes()[..], &bit_comm.0[..]].concat();
            let proof_hash = blake3_hash(&proof_msg);
            if !verify_scalar_signature(&proof_hash, bit_proof, &comm_point) {
                let comm_minus_h = comm_point - *h_generator();
                if !verify_scalar_signature(&proof_hash, bit_proof, &comm_minus_h) {
                    return false;
                }
            }
        }

        sum_point.compress().to_bytes() == self.commitment.0
    }
}

fn sign_with_scalar(message: &[u8; 32], scalar: &Scalar) -> [u8; 64] {
    let k = random_scalar();
    let k_g = EdwardsPoint::mul_base(&k);
    let k_g_bytes = k_g.compress().to_bytes();
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(message);
    combined.extend_from_slice(&k_g_bytes);
    let e_hash = blake3_hash(&combined);
    let e = scalar_from_hash(&e_hash);
    let s = k + e * scalar;
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&k_g_bytes);
    sig[32..].copy_from_slice(&s.to_bytes());
    sig
}

fn verify_scalar_signature(message: &[u8; 32], sig: &SignatureBytes, public_point: &EdwardsPoint) -> bool {
    let mut k_g_bytes = [0u8; 32];
    k_g_bytes.copy_from_slice(&sig.0[..32]);
    let k_g = match CompressedEdwardsY(k_g_bytes).decompress() {
        Some(p) => p,
        None => return false,
    };
    let s_bytes = {
        let mut b = [0u8; 32];
        b.copy_from_slice(&sig.0[32..]);
        b
    };
    let s = Scalar::from_bytes_mod_order(s_bytes);
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(message);
    combined.extend_from_slice(&k_g_bytes);
    let e_hash = blake3_hash(&combined);
    let e = scalar_from_hash(&e_hash);
    let s_g = EdwardsPoint::mul_base(&s);
    let e_p = e * public_point;
    s_g == k_g + e_p
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
    pub pseudo_out_commitment: PedersenCommitment,
    pub range_proofs: Vec<RangeProof>,
}

impl RingSignature {
    pub fn new(ring_size: u32) -> Self {
        RingSignature {
            ring_size,
            members: Vec::new(),
            signatures: Vec::new(),
            pseudo_out_commitment: PedersenCommitment([0u8; 32]),
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
        let n = members.len();
        let mut sig = RingSignature::new(n as u32);
        sig.members = members.to_vec();

        let mut ring_keys = Vec::with_capacity(n);
        for member in members {
            match CompressedEdwardsY(member.public_key.0).decompress() {
                Some(point) => ring_keys.push(point),
                None => return Err("Invalid ring member public key"),
            }
        }

        let msg_hash = blake3_hash(message);
        let mut s_vals = vec![Scalar::ZERO; n];
        let mut c_vals = vec![Scalar::ZERO; n];
        let mut challenge = Scalar::from_bytes_mod_order(msg_hash);

        let h_ki = h_key_image();
        let signer_sk = Scalar::from_bytes_mod_order(signer_secret.0);

        let k = if real_index == 0 {
            random_scalar()
        } else {
            Scalar::ZERO
        };

        for i in 0..n {
            let idx = (real_index + 1 + i) % n;
            if idx == real_index {
                let l = EdwardsPoint::mul_base(&k) + *h_ki * k;
                let mut combined = Vec::with_capacity(64);
                combined.extend_from_slice(&challenge.to_bytes());
                combined.extend_from_slice(&l.compress().to_bytes());
                challenge = scalar_from_hash(&blake3_hash(&combined));
                c_vals[idx] = challenge;
                s_vals[idx] = k - c_vals[idx] * signer_sk;
            } else {
                let fake_s = random_scalar();
                let fake_c = random_scalar();
                let l = EdwardsPoint::mul_base(&fake_s) + *h_ki * fake_s - fake_c * ring_keys[idx];
                let mut combined = Vec::with_capacity(64);
                combined.extend_from_slice(&challenge.to_bytes());
                combined.extend_from_slice(&l.compress().to_bytes());
                challenge = scalar_from_hash(&blake3_hash(&combined));
                s_vals[idx] = fake_s;
                c_vals[idx] = fake_c;
            }
        }

        for i in 0..n {
            let mut sig_bytes = [0u8; 64];
            sig_bytes[..32].copy_from_slice(&c_vals[i].to_bytes());
            sig_bytes[32..].copy_from_slice(&s_vals[i].to_bytes());
            sig.signatures.push(SignatureBytes(sig_bytes));
        }

        Ok(sig)
    }

    pub fn verify(&self, message: &[u8]) -> Result<(), &'static str> {
        let n = self.members.len();
        if n == 0 || n != self.signatures.len() {
            return Err("Invalid ring signature structure");
        }

        let msg_hash = blake3_hash(message);
        let mut challenge = Scalar::from_bytes_mod_order(msg_hash);
        let h_ki = h_key_image();

        for i in 0..n {
            let ring_key = match CompressedEdwardsY(self.members[i].public_key.0).decompress() {
                Some(p) => p,
                None => return Err("Invalid ring member key"),
            };

            let c = {
                let mut b = [0u8; 32];
                b.copy_from_slice(&self.signatures[i].0[..32]);
                Scalar::from_bytes_mod_order(b)
            };
            let s = {
                let mut b = [0u8; 32];
                b.copy_from_slice(&self.signatures[i].0[32..]);
                Scalar::from_bytes_mod_order(b)
            };

            let l = EdwardsPoint::mul_base(&s) + *h_ki * s - c * ring_key;
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(&challenge.to_bytes());
            combined.extend_from_slice(&l.compress().to_bytes());
            challenge = scalar_from_hash(&blake3_hash(&combined));
        }

        let expected_final = Scalar::from_bytes_mod_order(msg_hash);
        if challenge == expected_final {
            Ok(())
        } else {
            Err("Ring signature verification failed")
        }
    }
}

pub fn generate_key_image(private_key: &SecretKey) -> [u8; 32] {
    let sk = Scalar::from_bytes_mod_order(private_key.0);
    let pk = PublicKey::from_bytes(&private_key.0).unwrap();
    let h_p = hash_to_point(&pk.0);
    let key_image_point = sk * h_p;
    key_image_point.compress().to_bytes()
}

pub fn pedersen_commitment(amount: u64, blinding: &[u8; 32]) -> [u8; 32] {
    let scalar = Scalar::from_bytes_mod_order(*blinding);
    PedersenCommitment::new(amount, &scalar).0
}

pub fn verify_commitment(commitment: &[u8; 32], amount: u64, blinding: &[u8; 32]) -> bool {
    let scalar = Scalar::from_bytes_mod_order(*blinding);
    PedersenCommitment::verify(&PedersenCommitment(*commitment), amount, &scalar)
}

pub fn select_ring_members(
    utxo_set: &std::collections::HashMap<String, crate::chain::transaction::TxOutput>,
    real_pubkey: &PublicKey,
    count: usize,
) -> Vec<RingMember> {
    let mut members = Vec::new();
    let key_image = generate_key_image(&SecretKey::from_bytes(&real_pubkey.0).unwrap());
    members.push(RingMember {
        public_key: real_pubkey.clone(),
        key_image,
    });
    let decoys: Vec<&PublicKey> = utxo_set
        .values()
        .map(|o| &o.stealth_address.spend_pub)
        .filter(|pk| pk.0 != real_pubkey.0)
        .take(count.saturating_sub(1))
        .collect();
    for decoy in decoys {
        let dummy_ki = generate_key_image(&SecretKey::from_bytes(&decoy.0).unwrap());
        members.push(RingMember {
            public_key: decoy.clone(),
            key_image: dummy_ki,
        });
    }
    members
}
