//! RFC 7693 compression for the state used by `sodium_crypto_generichash_update($state, 'abc')`.

use zeroize::Zeroize;

pub(crate) const BLOCK_LEN: usize = 128;

pub(crate) const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

// RFC 7693 §2.7. Round numbers and message-word indices are public.
const SCHEDULE: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

pub(crate) fn compress(state: &mut [u64; 8], block: &[u8; BLOCK_LEN], count: u128, last: bool) {
    compress_rounds(state, block, count, last, 12);
}

fn compress_rounds(
    state: &mut [u64; 8],
    block: &[u8; BLOCK_LEN],
    count: u128,
    last: bool,
    rounds: usize,
) {
    let mut message = [0; 16];
    for (word, bytes) in message.iter_mut().zip(block.as_chunks::<8>().0) {
        *word = u64::from_le_bytes(*bytes);
    }
    let mut work = [0; 16];
    work[..8].copy_from_slice(state);
    work[8..].copy_from_slice(&IV);
    work[12] ^= count as u64;
    work[13] ^= (count >> 64) as u64;
    work[14] ^= 0_u64.wrapping_sub(u64::from(last));

    for schedule in SCHEDULE.iter().cycle().take(rounds) {
        mix(
            &mut work,
            [0, 4, 8, 12],
            message[schedule[0]],
            message[schedule[1]],
        );
        mix(
            &mut work,
            [1, 5, 9, 13],
            message[schedule[2]],
            message[schedule[3]],
        );
        mix(
            &mut work,
            [2, 6, 10, 14],
            message[schedule[4]],
            message[schedule[5]],
        );
        mix(
            &mut work,
            [3, 7, 11, 15],
            message[schedule[6]],
            message[schedule[7]],
        );
        mix(
            &mut work,
            [0, 5, 10, 15],
            message[schedule[8]],
            message[schedule[9]],
        );
        mix(
            &mut work,
            [1, 6, 11, 12],
            message[schedule[10]],
            message[schedule[11]],
        );
        mix(
            &mut work,
            [2, 7, 8, 13],
            message[schedule[12]],
            message[schedule[13]],
        );
        mix(
            &mut work,
            [3, 4, 9, 14],
            message[schedule[14]],
            message[schedule[15]],
        );
    }

    for (index, word) in state.iter_mut().enumerate() {
        *word ^= work[index] ^ work[index + 8];
    }
    work.zeroize();
    message.zeroize();
}

// RFC 7693 §3.1.
fn mix(work: &mut [u64; 16], [a, b, c, d]: [usize; 4], x: u64, y: u64) {
    work[a] = work[a].wrapping_add(work[b]).wrapping_add(x);
    work[d] = (work[d] ^ work[a]).rotate_right(32);
    work[c] = work[c].wrapping_add(work[d]);
    work[b] = (work[b] ^ work[c]).rotate_right(24);
    work[a] = work[a].wrapping_add(work[b]).wrapping_add(y);
    work[d] = (work[d] ^ work[a]).rotate_right(16);
    work[c] = work[c].wrapping_add(work[d]);
    work[b] = (work[b] ^ work[c]).rotate_right(63);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_7693_appendix_a() {
        let mut state = [
            0x6a09e667f2bdc948,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        let mut block = [0; BLOCK_LEN];
        block[..3].copy_from_slice(b"abc");
        compress(&mut state, &block, 3, true);
        assert_eq!(
            state,
            [
                0x0d4d1c983fa580ba,
                0xe9f6129fb697276a,
                0xb7c45a68142f214c,
                0xd1a2ffdb6fbb124b,
                0x2d79ab2a39c5877d,
                0x95cc3345ded552c2,
                0x5a92f1dba88ad318,
                0x239900d4ed8623b9,
            ],
        );
    }

    #[test]
    fn eip_152_vectors_4_through_7() {
        let state_bytes = base16ct::lower::decode_vec(concat!(
            "48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5",
            "d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b",
        ))
        .unwrap();
        let mut block = [0; BLOCK_LEN];
        block[..3].copy_from_slice(b"abc");
        let vectors = [
            (
                0,
                true,
                concat!(
                    "08c9bcf367e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5",
                    "d282e6ad7f520e511f6c3e2b8c68059b9442be0454267ce079217e1319cde05b",
                ),
            ),
            (
                12,
                true,
                concat!(
                    "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1",
                    "7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923",
                ),
            ),
            (
                12,
                false,
                concat!(
                    "75ab69d3190a562c51aef8d88f1c2775876944407270c42c9844252c26d287529",
                    "8743e7f6d5ea2f2d3e8d226039cd31b4e426ac4f2d3d666a610c2116fde4735",
                ),
            ),
            (
                1,
                true,
                concat!(
                    "b63a380cb2897d521994a85234ee2c181b5f844d2c624c002677e9703449d2fba",
                    "551b3a8333bcdf5f2f7e08993d53923de3d64fcc68c034e717b9293fed7a421",
                ),
            ),
        ];
        for (rounds, last, expected) in vectors {
            let mut state = [0; 8];
            for (word, bytes) in state.iter_mut().zip(state_bytes.as_chunks::<8>().0) {
                *word = u64::from_le_bytes(*bytes);
            }
            compress_rounds(&mut state, &block, 3, last, rounds);
            let actual = state.map(u64::to_le_bytes);
            assert_eq!(
                base16ct::lower::encode_string(actual.as_flattened()),
                expected
            );
        }
    }
}
