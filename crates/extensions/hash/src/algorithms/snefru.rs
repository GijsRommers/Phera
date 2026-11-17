//! `hash('snefru', 'abc')` uses the eight-pass, 256-bit Snefru variant.
//! Ported from php-src/ext/hash/hash_snefru.c (BSD-3-Clause, see LICENSE-PHP).
use super::SNEFRU_TABLES;

pub fn hash(input: &[u8]) -> Vec<u8> {
    let mut state = [0u32; 16];
    for block in input.chunks(32) {
        let mut padded = [0; 32];
        padded[..block.len()].copy_from_slice(block);
        for (word, bytes) in state[8..].iter_mut().zip(padded.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        compress(&mut state);
        state[8..].fill(0);
    }
    let length = input.len() as u64 * 8;
    state[14] = (length >> 32) as u32;
    state[15] = length as u32;
    compress(&mut state);
    state[..8]
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .collect()
}

fn compress(state: &mut [u32; 16]) {
    let mut work = *state;
    for pass in 0..8 {
        for shift in [16, 8, 16, 24] {
            for i in 0..16 {
                let value = SNEFRU_TABLES[pass * 2 + ((i / 2) & 1)][(work[i] & 255) as usize];
                work[(i + 15) & 15] ^= value;
                work[(i + 1) & 15] ^= value;
            }
            for word in &mut work {
                *word = word.rotate_right(shift);
            }
        }
    }
    for i in 0..8 {
        state[i] ^= work[15 - i];
    }
}
