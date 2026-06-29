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

    let mut timestamps: Vec<u64> = window_blocks.iter().map(|b| b.header.timestamp).collect();
    timestamps.sort_unstable();
    let mid = timestamps.len() / 2;
    let median_first = if mid > 0 { timestamps[mid / 2] } else { timestamps[0] };
    let median_last = if mid > 0 { timestamps[mid + mid / 2] } else { timestamps[timestamps.len() - 1] };
    let time_span = median_last.saturating_sub(median_first).max(1);

    let expected_time = config::DIFFICULTY_TARGET_SECONDS * (window_blocks.len() as u64 - 1);

    let mut total_diff = 0u128;
    for block in window_blocks {
        total_diff += compact_to_difficulty(block.header.difficulty_target) as u128;
    }
    let avg_diff = (total_diff / window_blocks.len() as u128) as u64;

    let adjustment = (time_span as f64 / expected_time as f64).clamp(0.25, 4.0);
    (avg_diff as f64 / adjustment) as u64
}

pub fn difficulty_to_compact(difficulty: u64) -> u32 {
    if difficulty == 0 {
        return 0x1e00ffff;
    }
    let target = u64::MAX / difficulty;
    if target == 0 {
        return 0x1e00ffff;
    }
    let leading_zeros = target.leading_zeros() as u32;
    let exponent = (31 - leading_zeros) + 1;
    let mantissa = target >> (exponent - 3) * 8;
    (exponent << 24) | (mantissa as u32 & 0x00ffffff)
}

pub fn compact_to_target(compact: u32) -> u64 {
    let exponent = compact >> 24;
    let mantissa = compact & 0x00ffffff;
    if exponent <= 3 {
        (mantissa as u64) >> ((3 - exponent) * 8)
    } else if exponent > 11 {
        u64::MAX
    } else {
        (mantissa as u64) << ((exponent - 3) * 8)
    }
}

fn compact_to_difficulty(compact: u32) -> u64 {
    let target = compact_to_target(compact);
    if target == 0 {
        return u64::MAX;
    }
    u64::MAX / target
}
