use crate::chain::block::Block;

pub fn check_pow(block: &Block, target: u64) -> bool {
    let hash = block.miner_hash();
    let hash_val = u64::from_le_bytes(hash[24..32].try_into().unwrap_or([0u8; 8]));
    hash_val <= target
}

pub fn mine_block(block: &mut Block, target: u64, max_nonces: u64) -> Option<u64> {
    for nonce in 0..max_nonces {
        block.set_nonce(nonce, 0);
        if check_pow(block, target) {
            return Some(nonce);
        }
    }
    None
}

pub fn calculate_target(difficulty: u64) -> u64 {
    if difficulty == 0 {
        return u64::MAX;
    }
    u64::MAX / difficulty
}
