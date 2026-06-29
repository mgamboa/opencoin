use ed25519_dalek::{
    Signature, Signer, SigningKey, Verifier, VerifyingKey,
};
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::{SerializeTuple, Serializer};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const PUBLIC_KEY_SIZE: usize = 32;
pub const SECRET_KEY_SIZE: usize = 32;
pub const SIGNATURE_SIZE: usize = 64;

#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey(pub [u8; PUBLIC_KEY_SIZE]);

#[derive(Zeroize, ZeroizeOnDrop, Clone)]
pub struct SecretKey(pub [u8; SECRET_KEY_SIZE]);

#[derive(Clone)]
pub struct KeyPair {
    pub public: PublicKey,
    pub secret: SecretKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignatureBytes(pub [u8; SIGNATURE_SIZE]);

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", hex::encode(self.0))
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey([***])")
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("public", &self.public)
            .field("secret", &self.secret)
            .finish()
    }
}

impl fmt::Debug for SignatureBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignatureBytes({})", hex::encode(self.0))
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_tuple(PUBLIC_KEY_SIZE)?;
        for byte in &self.0 {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ArrayVisitor;
        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = PublicKey;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a 32-byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<PublicKey, A::Error> {
                let mut arr = [0u8; PUBLIC_KEY_SIZE];
                for (i, byte) in arr.iter_mut().enumerate() {
                    *byte = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(PublicKey(arr))
            }
        }
        deserializer.deserialize_tuple(PUBLIC_KEY_SIZE, ArrayVisitor)
    }
}

impl Serialize for SecretKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_tuple(SECRET_KEY_SIZE)?;
        for byte in &self.0 {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ArrayVisitor;
        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = SecretKey;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a 32-byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<SecretKey, A::Error> {
                let mut arr = [0u8; SECRET_KEY_SIZE];
                for (i, byte) in arr.iter_mut().enumerate() {
                    *byte = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(SecretKey(arr))
            }
        }
        deserializer.deserialize_tuple(SECRET_KEY_SIZE, ArrayVisitor)
    }
}

impl Serialize for SignatureBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_tuple(SIGNATURE_SIZE)?;
        for byte in &self.0 {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ArrayVisitor;
        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = SignatureBytes;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a 64-byte array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<SignatureBytes, A::Error> {
                let mut arr = [0u8; SIGNATURE_SIZE];
                for (i, byte) in arr.iter_mut().enumerate() {
                    *byte = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(SignatureBytes(arr))
            }
        }
        deserializer.deserialize_tuple(SIGNATURE_SIZE, ArrayVisitor)
    }
}

impl PublicKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != PUBLIC_KEY_SIZE {
            return Err("Invalid public key length");
        }
        let mut arr = [0u8; PUBLIC_KEY_SIZE];
        arr.copy_from_slice(bytes);
        Ok(PublicKey(arr))
    }

    pub fn to_verifying_key(&self) -> Result<VerifyingKey, ed25519_dalek::SignatureError> {
        VerifyingKey::from_bytes(&self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, &'static str> {
        let bytes = hex::decode(s).map_err(|_| "Invalid hex")?;
        Self::from_bytes(&bytes)
    }
}

impl SecretKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != SECRET_KEY_SIZE {
            return Err("Invalid secret key length");
        }
        let mut arr = [0u8; SECRET_KEY_SIZE];
        arr.copy_from_slice(bytes);
        Ok(SecretKey(arr))
    }

    pub fn to_signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.0)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

fn fill_random_bytes(buf: &mut [u8]) {
    getrandom::fill(buf).expect("Failed to generate random bytes");
}

impl KeyPair {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        fill_random_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        KeyPair {
            public: PublicKey(verifying_key.to_bytes()),
            secret: SecretKey(signing_key.to_bytes()),
        }
    }

    pub fn from_secret_key(secret: &SecretKey) -> Self {
        let signing_key = secret.to_signing_key();
        let verifying_key = signing_key.verifying_key();
        KeyPair {
            public: PublicKey(verifying_key.to_bytes()),
            secret: secret.clone(),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Result<SignatureBytes, &'static str> {
        let signing_key = self.secret.to_signing_key();
        let signature = signing_key.sign(message);
        Ok(SignatureBytes(signature.to_bytes()))
    }
}

impl SignatureBytes {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != SIGNATURE_SIZE {
            return Err("Invalid signature length");
        }
        let mut arr = [0u8; SIGNATURE_SIZE];
        arr.copy_from_slice(bytes);
        Ok(SignatureBytes(arr))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn verify(&self, message: &[u8], public_key: &PublicKey) -> Result<(), &'static str> {
        let verifying_key = public_key.to_verifying_key().map_err(|_| "Invalid key")?;
        let sig = Signature::from_bytes(&self.0);
        verifying_key.verify(message, &sig).map_err(|_| "Invalid signature")
    }
}

pub fn generate_keypair() -> KeyPair {
    KeyPair::generate()
}

pub fn public_key_to_address(public_key: &PublicKey) -> String {
    let mut address = String::from("OC");
    address.push_str(&hex::encode(public_key.0));
    let checksum = crate::crypto::hash::double_sha3_256(address.as_bytes());
    address.push_str(&hex::encode(&checksum[..4]));
    address
}
