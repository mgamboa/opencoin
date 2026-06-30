use sha3::{Digest, Sha3_256, Sha3_512};

pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

pub fn sha3_512(data: &[u8]) -> [u8; 64] {
    let mut hasher = Sha3_512::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 64];
    out.copy_from_slice(&result);
    out
}

pub fn double_sha3_256(data: &[u8]) -> [u8; 32] {
    sha3_256(&sha3_256(data))
}

pub fn hash_to_scalar(data: &[u8]) -> [u8; 32] {
    blake3_hash(data)
}

fn combine_hashes(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut combined = [0u8; 64];
    combined[..32].copy_from_slice(a);
    combined[32..].copy_from_slice(b);
    blake3_hash(&combined)
}

pub fn merkle_proof(hashes: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
    if hashes.is_empty() || index >= hashes.len() {
        return Vec::new();
    }
    let mut proof = Vec::new();
    let mut level: Vec<[u8; 32]> = hashes.to_vec();
    let mut idx = index;
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        for chunk in level.chunks(2) {
            if chunk.len() == 2 {
                next.push(combine_hashes(&chunk[0], &chunk[1]));
            } else {
                next.push(chunk[0]);
            }
        }
        let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        if sibling_idx < level.len() {
            proof.push(level[sibling_idx]);
        }
        idx /= 2;
        level = next;
    }
    proof
}

pub fn verify_merkle_proof(leaf: &[u8; 32], proof: &[[u8; 32]], index: usize, root: &[u8; 32]) -> bool {
    let mut current = *leaf;
    let mut idx = index;
    for sibling in proof {
        current = if idx % 2 == 0 {
            combine_hashes(&current, sibling)
        } else {
            combine_hashes(sibling, &current)
        };
        idx /= 2;
    }
    current == *root
}

pub fn merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    if hashes.is_empty() {
        return [0u8; 32];
    }
    if hashes.len() == 1 {
        return hashes[0];
    }
    let mut next_level = Vec::with_capacity((hashes.len() + 1) / 2);
    for chunk in hashes.chunks(2) {
        if chunk.len() == 2 {
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(&chunk[0]);
            combined.extend_from_slice(&chunk[1]);
            next_level.push(blake3_hash(&combined));
        } else {
            next_level.push(chunk[0]);
        }
    }
    merkle_root(&next_level)
}
