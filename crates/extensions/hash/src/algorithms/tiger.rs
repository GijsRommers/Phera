//! `hash('tiger192,4', 'abc')` adds a fourth Tiger pass and truncates afterward.
//! Ported from php-src/ext/hash/hash_tiger.c (BSD-3-Clause, see LICENSE-PHP).
mod table_0;
mod table_1;
mod table_2;
mod table_3;

const TABLES: [&[u64; 256]; 4] = [
    &table_0::TABLE,
    &table_1::TABLE,
    &table_2::TABLE,
    &table_3::TABLE,
];

pub fn hash(input: &[u8], bits: usize, passes: usize) -> Vec<u8> {
    let mut data = input.to_vec();
    data.push(1);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&(input.len() as u64 * 8).to_le_bytes());
    let mut state = [0x0123456789abcdef, 0xfedcba9876543210, 0xf096a5b4c3b2e187];
    for block in data.as_chunks::<64>().0 {
        let mut words = [0u64; 8];
        for (word, bytes) in words.iter_mut().zip(block.as_chunks::<8>().0) {
            *word = u64::from_le_bytes(*bytes);
        }
        let mut work = state;
        pass(&mut work, &words, 5, 0);
        schedule(&mut words);
        pass(&mut work, &words, 7, 2);
        schedule(&mut words);
        pass(&mut work, &words, 9, 1);
        if passes == 4 {
            schedule(&mut words);
            pass(&mut work, &words, 9, 0);
            work.rotate_right(1);
        }
        state[0] ^= work[0];
        state[1] = work[1].wrapping_sub(state[1]);
        state[2] = work[2].wrapping_add(state[2]);
    }
    state
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .take(bits / 8)
        .collect()
}

fn pass(state: &mut [u64; 3], words: &[u64; 8], multiplier: u64, start: usize) {
    let [t1, t2, t3, t4] = TABLES;
    for (i, word) in words.iter().enumerate() {
        let a = (start + i) % 3;
        let b = (a + 1) % 3;
        let c = (a + 2) % 3;
        state[c] ^= word;
        let bytes = state[c].to_le_bytes().map(usize::from);
        state[a] = state[a].wrapping_sub(t1[bytes[0]] ^ t2[bytes[2]] ^ t3[bytes[4]] ^ t4[bytes[6]]);
        state[b] = state[b]
            .wrapping_add(t4[bytes[1]] ^ t3[bytes[3]] ^ t2[bytes[5]] ^ t1[bytes[7]])
            .wrapping_mul(multiplier);
    }
}

fn schedule(x: &mut [u64; 8]) {
    x[0] = x[0].wrapping_sub(x[7] ^ 0xa5a5a5a5a5a5a5a5);
    x[1] ^= x[0];
    x[2] = x[2].wrapping_add(x[1]);
    x[3] = x[3].wrapping_sub(x[2] ^ ((!x[1]) << 19));
    x[4] ^= x[3];
    x[5] = x[5].wrapping_add(x[4]);
    x[6] = x[6].wrapping_sub(x[5] ^ ((!x[4]) >> 23));
    x[7] ^= x[6];
    x[0] = x[0].wrapping_add(x[7]);
    x[1] = x[1].wrapping_sub(x[0] ^ ((!x[7]) << 19));
    x[2] ^= x[1];
    x[3] = x[3].wrapping_add(x[2]);
    x[4] = x[4].wrapping_sub(x[3] ^ ((!x[2]) >> 23));
    x[5] ^= x[4];
    x[6] = x[6].wrapping_add(x[5]);
    x[7] = x[7].wrapping_sub(x[6] ^ 0x0123456789abcdef);
}
