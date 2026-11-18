//! `hash('snefru', 'abc')` uses the eight-pass, 256-bit Snefru variant.
//! Ported from php-src/ext/hash/hash_snefru.c (BSD-3-Clause, see LICENSE-PHP).
mod table_0;
mod table_1;
mod table_10;
mod table_11;
mod table_12;
mod table_13;
mod table_14;
mod table_15;
mod table_2;
mod table_3;
mod table_4;
mod table_5;
mod table_6;
mod table_7;
mod table_8;
mod table_9;

const TABLES: [&[u32; 256]; 16] = [
    &table_0::TABLE,
    &table_1::TABLE,
    &table_2::TABLE,
    &table_3::TABLE,
    &table_4::TABLE,
    &table_5::TABLE,
    &table_6::TABLE,
    &table_7::TABLE,
    &table_8::TABLE,
    &table_9::TABLE,
    &table_10::TABLE,
    &table_11::TABLE,
    &table_12::TABLE,
    &table_13::TABLE,
    &table_14::TABLE,
    &table_15::TABLE,
];

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
                let value = TABLES[pass * 2 + ((i / 2) & 1)][(work[i] & 255) as usize];
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
