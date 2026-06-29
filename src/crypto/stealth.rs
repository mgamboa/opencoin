use curve25519_dalek::{
    scalar::Scalar,
    edwards::{CompressedEdwardsY, EdwardsPoint},
};
use serde::{Deserialize, Serialize};

use super::hash::blake3_hash;
use super::keys::{PublicKey, SecretKey};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StealthAddress {
    pub spend_pub: PublicKey,
    pub view_pub: PublicKey,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EphemeralPublicKey(pub PublicKey);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct KeyImage(pub [u8; 32]);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OneTimeOutput {
    pub ephemeral_pub: EphemeralPublicKey,
    pub key_image: KeyImage,
    pub amount_commitment: [u8; 32],
}

impl StealthAddress {
    pub fn new(spend_key: &PublicKey, view_key: &PublicKey) -> Self {
        StealthAddress {
            spend_pub: spend_key.clone(),
            view_pub: view_key.clone(),
        }
    }

    pub fn from_keypair(keypair: &super::keys::KeyPair) -> Self {
        StealthAddress {
            spend_pub: keypair.public.clone(),
            view_pub: keypair.public.clone(),
        }
    }
}

fn generate_random_scalar() -> Scalar {
    let mut bytes = [0u8; 64];
    getrandom::fill(&mut bytes).expect("Failed to generate random bytes");
    Scalar::from_bytes_mod_order_wide(&bytes)
}

pub fn create_stealth_output(
    recipient: &StealthAddress,
    amount: u64,
) -> (OneTimeOutput, Scalar) {
    let r = generate_random_scalar();
    let rG = EdwardsPoint::mul_base(&r);

    let spend_point = CompressedEdwardsY(recipient.spend_pub.0)
        .decompress()
        .unwrap();
    let view_point = CompressedEdwardsY(recipient.view_pub.0)
        .decompress()
        .unwrap();

    let dh_shared = r * view_point;
    let shared_hash = blake3_hash(dh_shared.compress().as_bytes());
    let shared_scalar = Scalar::from_bytes_mod_order(shared_hash);

    let one_time_public = spend_point + EdwardsPoint::mul_base(&shared_scalar);

    let one_time_pub_bytes = one_time_public.compress().to_bytes();

    let key_image_preimage = blake3_hash(&one_time_pub_bytes);
    let key_image = KeyImage(key_image_preimage);

    let amount_commitment = blake3_hash(&amount.to_le_bytes());

    let output = OneTimeOutput {
        ephemeral_pub: EphemeralPublicKey(PublicKey(rG.compress().to_bytes())),
        key_image,
        amount_commitment,
    };

    (output, r)
}

pub fn recover_stealth_output(
    private_view: &SecretKey,
    output: &OneTimeOutput,
    recipient_stealth: &StealthAddress,
) -> Option<PublicKey> {
    let view_sk = Scalar::from_bytes_mod_order(private_view.0);
    let rG = CompressedEdwardsY(output.ephemeral_pub.0 .0)
        .decompress()
        .unwrap();

    let dh_shared = view_sk * rG;
    let shared_hash = blake3_hash(dh_shared.compress().as_bytes());
    let shared_scalar = Scalar::from_bytes_mod_order(shared_hash);

    let spend_point = CompressedEdwardsY(recipient_stealth.spend_pub.0)
        .decompress()
        .unwrap();
    let one_time_public = spend_point + EdwardsPoint::mul_base(&shared_scalar);

    Some(PublicKey(one_time_public.compress().to_bytes()))
}
