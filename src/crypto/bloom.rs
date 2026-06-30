use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomFilter {
    bits: Vec<u8>,
    num_hashes: u32,
    num_items: u32,
}

impl BloomFilter {
    pub fn new(expected_items: u32, false_positive_rate: f64) -> Self {
        let bits_len = (-(expected_items as f64) * false_positive_rate.ln()
            / (std::f64::consts::LN_2.powi(2))).ceil() as usize;
        let bits_len = (bits_len + 7) / 8 * 8;
        let bits_len = bits_len.max(8);
        let num_hashes = ((bits_len as f64 / expected_items as f64) * std::f64::consts::LN_2).ceil() as u32;
        let num_hashes = num_hashes.max(1).min(50);
        BloomFilter {
            bits: vec![0u8; bits_len / 8],
            num_hashes,
            num_items: 0,
        }
    }

    fn hash(&self, data: &[u8], seed: u32) -> usize {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&seed.to_le_bytes());
        hasher.update(data);
        let hash = hasher.finalize();
        let val = u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap());
        (val as usize) % (self.bits.len() * 8)
    }

    pub fn insert(&mut self, data: &[u8]) {
        for i in 0..self.num_hashes {
            let bit = self.hash(data, i);
            self.bits[bit / 8] |= 1 << (bit % 8);
        }
        self.num_items += 1;
    }

    pub fn contains(&self, data: &[u8]) -> bool {
        for i in 0..self.num_hashes {
            let bit = self.hash(data, i);
            if self.bits[bit / 8] & (1 << (bit % 8)) == 0 {
                return false;
            }
        }
        true
    }

    pub fn insert_address(&mut self, addr: &[u8; 32]) {
        self.insert(addr);
    }

    pub fn insert_tx_hash(&mut self, tx_hash: &[u8; 32]) {
        self.insert(tx_hash);
    }

    pub fn matches(&self, data: &[u8]) -> bool {
        self.contains(data)
    }
}
