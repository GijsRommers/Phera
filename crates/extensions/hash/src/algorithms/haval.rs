//! `hash('haval128,3', '')`: HAVAL's pass count and output size alter its rounds.
//! Ported from php-src/ext/hash/hash_haval.c (BSD-3-Clause, see LICENSE-PHP).
mod constants;
mod fold;

use constants::{D0, I2, I3, I4, I5, K2, K3, K4, K5};

pub fn hash(input: &[u8], bits: usize, passes: usize) -> Vec<u8> {
    let mut data = input.to_vec();
    data.push(1);
    while data.len() % 128 != 118 {
        data.push(0);
    }
    data.push(1 | ((passes as u8) << 3));
    data.push((bits >> 2) as u8);
    data.extend_from_slice(&(input.len() as u64 * 8).to_le_bytes());
    let mut state = D0;
    for block in data.as_chunks::<128>().0 {
        compress(&mut state, block, passes);
    }
    fold::fold(&mut state, bits);
    state
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .take(bits / 8)
        .collect()
}

fn boolean(pass: usize, [x6, x5, x4, x3, x2, x1, x0]: [u32; 7]) -> u32 {
    match pass {
        0 => (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x1) ^ x0,
        1 => {
            (x1 & x2 & x3)
                ^ (x2 & x4 & x5)
                ^ (x1 & x2)
                ^ (x1 & x4)
                ^ (x2 & x6)
                ^ (x3 & x5)
                ^ (x4 & x5)
                ^ (x0 & x2)
                ^ x0
        }
        2 => (x1 & x2 & x3) ^ (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x3) ^ x0,
        3 => {
            (x1 & x2 & x3)
                ^ (x2 & x4 & x5)
                ^ (x3 & x4 & x6)
                ^ (x1 & x4)
                ^ (x2 & x6)
                ^ (x3 & x4)
                ^ (x3 & x5)
                ^ (x3 & x6)
                ^ (x4 & x5)
                ^ (x4 & x6)
                ^ (x0 & x4)
                ^ x0
        }
        _ => (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x1 & x2 & x3) ^ (x0 & x5) ^ x0,
    }
}

fn compress(state: &mut [u32; 8], block: &[u8], passes: usize) {
    let permutations: &[[usize; 7]] = match passes {
        3 => &[
            [1, 0, 3, 5, 6, 2, 4],
            [4, 2, 1, 0, 5, 3, 6],
            [6, 1, 2, 3, 4, 5, 0],
        ],
        4 => &[
            [2, 6, 1, 4, 5, 3, 0],
            [3, 5, 2, 0, 1, 6, 4],
            [1, 4, 3, 6, 0, 2, 5],
            [6, 4, 0, 5, 2, 1, 3],
        ],
        _ => &[
            [3, 4, 1, 0, 5, 2, 6],
            [6, 2, 1, 0, 3, 4, 5],
            [2, 6, 0, 4, 3, 1, 5],
            [1, 5, 3, 2, 0, 4, 6],
            [2, 5, 0, 6, 4, 3, 1],
        ],
    };
    let mut words = [0u32; 32];
    for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*bytes);
    }
    let indices = [&I2, &I3, &I4, &I5];
    let constants = [&K2, &K3, &K4, &K5];
    let mut work = *state;
    for (pass, permutation) in permutations.iter().enumerate() {
        for i in 0..32 {
            let at = (7usize.wrapping_sub(i)) & 7;
            let args = permutation.map(|j| work[(j + 32 - i) & 7]);
            let (word, constant) = if pass == 0 {
                (words[i], 0)
            } else {
                (words[indices[pass - 1][i]], constants[pass - 1][i])
            };
            work[at] = boolean(pass, args)
                .rotate_right(7)
                .wrapping_add(work[at].rotate_right(11))
                .wrapping_add(word)
                .wrapping_add(constant);
        }
    }
    for (value, result) in state.iter_mut().zip(work) {
        *value = value.wrapping_add(result);
    }
}
