use crate::crypto::keys::{PublicKey, public_key_to_address, SecretKey};
use crate::crypto::stealth::StealthAddress;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCoinAddress {
    pub spend_key: PublicKey,
    pub view_key: PublicKey,
    pub address_str: String,
}

impl OpenCoinAddress {
    pub fn new(spend_key: &PublicKey, view_key: &PublicKey) -> Self {
        let spend_str = public_key_to_address(spend_key);
        OpenCoinAddress {
            spend_key: spend_key.clone(),
            view_key: view_key.clone(),
            address_str: format!("OC{}", &spend_str[2..]),
        }
    }

    pub fn from_stealth(stealth: &StealthAddress) -> Self {
        let spend_str = public_key_to_address(&stealth.spend_pub);
        OpenCoinAddress {
            spend_key: stealth.spend_pub.clone(),
            view_key: stealth.view_pub.clone(),
            address_str: format!("OC{}", &spend_str[2..]),
        }
    }

    pub fn to_stealth(&self) -> StealthAddress {
        StealthAddress {
            spend_pub: self.spend_key.clone(),
            view_pub: self.view_key.clone(),
        }
    }

    pub fn to_string(&self) -> String {
        self.address_str.clone()
    }

    pub fn from_string(s: &str) -> Result<Self, &'static str> {
        if !s.starts_with("OC") || s.len() != 76 {
            return Err("Invalid address format");
        }
        let pub_hex = &s[2..66];
        let _checksum_hex = &s[66..74];
        let pub_bytes = hex::decode(pub_hex).map_err(|_| "Invalid hex")?;
        let public_key = PublicKey::from_bytes(&pub_bytes)?;
        Ok(OpenCoinAddress {
            spend_key: public_key.clone(),
            view_key: public_key.clone(),
            address_str: s.to_string(),
        })
    }
}

pub fn validate_address(address: &str) -> bool {
    OpenCoinAddress::from_string(address).is_ok()
}
