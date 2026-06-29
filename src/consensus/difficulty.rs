use crate::config;
use crate::chain::block::Block;

pub fn calculate_difficulty(blocks: &[Block]) -> u64 {
    if blocks.is_empty() || blocks.len() < 2 {
        return 1;
    }

    let window = config::DIFFICULTY_WINDOW as usize;
    let start = if blocks.len() > window { blocks.len() - window } else { 0 };
    let window_blocks = &blocks[start..];

    if window_blocks.len() < 2 {
        return 1;
    }

    let first_ts = window_blocks[0].header.timestamp;
    let last_ts = window_blocks[window_blocks.len() - 1].header.timestamp;
    let time_span = last_ts.saturating_sub(first_ts);

    let expected_time = config::DIFFICULTY_TARGET_SECONDS * (window_blocks.len() as u64 - 1);
    if time_span == 0 {
        return 1;
    }

    let mut difficulty: u64 = 1;
    for block in window_blocks {
        let compact = block.header.difficulty_target as u64;
        let exponent = compact >> 24;
        let mantissa = compact & 0x00ffffff;
        if exponent <= 3 {
            continue;
        }
        let block_diff = (0x0000ffffu64 << (exponent - 3) * 8).saturating_div(mantissa | 0x008000);
        difficulty = difficulty.saturating_add(block_diff);
    }
    difficulty = difficulty / (window_blocks.len() as u64);
    if difficulty == 0 {
        difficulty = 1;
    }

    let adjustment = (time_span as f64 / expected_time as f64).clamp(0.25, 4.0);
    (difficulty as f64 * adjustment) as u64
}

pub fn difficulty_to_target(difficulty: u64) -> u32 {
    if difficulty == 0 {
        return 0x1e00ffff;
    }
    let target = u64::MAX / difficulty;
    let target_bytes = target.to_be_bytes();
    let leading_zeros = target_bytes.iter().take_while(|&&b| b == 0).count();
    if leading_zeros >= 32 {
        return 0x1e00ffff;
    }
    let mut compact = ((leading_zeros as u32 + 1) << 24);
    if leading_zeros < 32 {
        compact |= (target_bytes[leading_zeros] as u32) << 16;
    }
    if leading_zeros + 1 < 32 {
        compact |= (target_bytes[leading_zeros + 1] as u32) << 8;
    }
    if leading_zeros + 2 < 32 {
        compact |= target_bytes[leading_zeros + 2] as u32;
    }
    compact
}
