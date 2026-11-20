//! `sodium_crypto_generichash_update($state, $chunk)` retains only one BLAKE2b block.

use blake2b_simd::Params;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::blake2b_compress::{BLOCK_LEN, IV, compress};

pub(crate) const STATE_LEN: usize = 384;
const MAGIC: &[u8; 8] = b"PHERAGH1";
const CHECKSUM_OFFSET: usize = STATE_LEN - 32;

pub(crate) struct BlakeState {
    words: [u64; 8],
    compressed: u128,
    pending: [u8; BLOCK_LEN],
    used: usize,
}

impl BlakeState {
    pub(crate) fn new(key: &[u8], length: usize) -> Self {
        let mut words = IV;
        // Sequential BLAKE2b uses depth and fanout 1, with optional keying.
        words[0] ^= u64::from_le_bytes([length as u8, key.len() as u8, 1, 1, 0, 0, 0, 0]);
        let mut pending = [0; BLOCK_LEN];
        pending[..key.len()].copy_from_slice(key);
        Self {
            words,
            compressed: 0,
            pending,
            used: if key.is_empty() { 0 } else { BLOCK_LEN },
        }
    }

    pub(crate) fn decode(input: &[u8; STATE_LEN]) -> Option<Self> {
        let (payload, check) = input.split_at(CHECKSUM_OFFSET);
        if &payload[..8] != MAGIC || !bool::from(checksum(payload).as_bytes().ct_eq(check)) {
            return None;
        }
        let compressed = u128::from_le_bytes(payload[72..88].try_into().ok()?);
        let used = usize::from(payload[88]);
        if used > BLOCK_LEN || !compressed.is_multiple_of(BLOCK_LEN as u128) {
            return None;
        }
        let mut words = [0; 8];
        for (word, chunk) in words.iter_mut().zip(payload[8..72].as_chunks::<8>().0) {
            *word = u64::from_le_bytes(*chunk);
        }
        Some(Self {
            words,
            compressed,
            used,
            pending: payload[89..217].try_into().ok()?,
        })
    }

    pub(crate) fn encode(&self) -> [u8; STATE_LEN] {
        let mut output = [0; STATE_LEN];
        output[..8].copy_from_slice(MAGIC);
        for (dst, word) in output[8..72]
            .as_chunks_mut::<8>()
            .0
            .iter_mut()
            .zip(self.words)
        {
            *dst = word.to_le_bytes();
        }
        output[72..88].copy_from_slice(&self.compressed.to_le_bytes());
        output[88] = self.used as u8;
        output[89..89 + self.used].copy_from_slice(&self.pending[..self.used]);
        let check = checksum(&output[..CHECKSUM_OFFSET]);
        output[CHECKSUM_OFFSET..].copy_from_slice(check.as_bytes());
        output
    }

    pub(crate) fn update(&mut self, mut input: &[u8]) -> Option<()> {
        let space = BLOCK_LEN - self.used;
        if input.len() > space {
            let (prefix, tail) = input.split_at(space);
            self.pending[self.used..].copy_from_slice(prefix);
            self.compressed = self.compressed.checked_add(BLOCK_LEN as u128)?;
            compress(&mut self.words, &self.pending, self.compressed, false);
            self.pending.zeroize();
            self.used = 0;
            input = tail;
            // BLAKE2b needs even a full last block until the final flag is known.
            while input.len() > BLOCK_LEN {
                let (block, tail) = input.split_first_chunk::<BLOCK_LEN>()?;
                self.compressed = self.compressed.checked_add(BLOCK_LEN as u128)?;
                compress(&mut self.words, block, self.compressed, false);
                input = tail;
            }
        }
        self.pending[self.used..self.used + input.len()].copy_from_slice(input);
        self.used += input.len();
        Some(())
    }

    pub(crate) fn finish(mut self) -> Option<[u8; 64]> {
        let total = self.compressed.checked_add(self.used as u128)?;
        self.pending[self.used..].fill(0);
        compress(&mut self.words, &self.pending, total, true);
        let mut digest = [0; 64];
        for (dst, word) in digest.as_chunks_mut::<8>().0.iter_mut().zip(self.words) {
            *dst = word.to_le_bytes();
        }
        Some(digest)
    }
}

impl Drop for BlakeState {
    fn drop(&mut self) {
        self.words.zeroize();
        self.pending.zeroize();
    }
}

fn checksum(payload: &[u8]) -> blake2b_simd::Hash {
    Params::new()
        .hash_length(32)
        .personal(b"Phera GH state")
        .hash(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_matches_blake2b_simd_one_shot() {
        let mut cases = 0;
        let mut data = (1_u64..)
            .map(|seed| {
                let mut word = seed.wrapping_mul(0x9e3779b97f4a7c15);
                word ^= word >> 31;
                word ^= word << 17;
                word as u8
            })
            .take(2 * 1024 * 1024 + 129)
            .collect::<Vec<_>>();
        for key_len in [0, 64] {
            for salt_len in [0, 16] {
                for personal_len in [0, 16] {
                    let key = &b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        [..key_len];
                    let salt = &b"test salt bytes!"[..salt_len];
                    let personal = &b"streaming BLAKE2"[..personal_len];
                    for length in (0..=257).chain([data.len()]) {
                        for digest_len in [16 + length % 48, 64] {
                            let input = &data[..length];
                            let expected = Params::new()
                                .hash_length(digest_len)
                                .key(key)
                                .salt(salt)
                                .personal(personal)
                                .hash(input);
                            let mut state = BlakeState::new(key, digest_len);
                            // These parameter words are used here to exercise F.
                            // PHP's streaming API takes only a key and digest length.
                            let mut params = [0_u8; 32];
                            params[..salt_len].copy_from_slice(salt);
                            params[16..16 + personal_len].copy_from_slice(personal);
                            for (word, field) in
                                state.words[4..].iter_mut().zip(params.as_chunks::<8>().0)
                            {
                                *word ^= u64::from_le_bytes(*field);
                            }
                            let split = if digest_len == 64 {
                                length.min(BLOCK_LEN)
                            } else {
                                length / 3
                            };
                            let (first, rest) = input.split_at(split);
                            state.update(first).unwrap();
                            let mut state = BlakeState::decode(&state.encode()).unwrap();
                            for chunk in rest.chunks(1 + length % 131) {
                                state.update(chunk).unwrap();
                            }
                            assert_eq!(
                                &state.finish().unwrap()[..digest_len],
                                expected.as_bytes(),
                                "input {length}, output {digest_len}, key {key_len}, \
                                 salt {salt_len}, personalisation {personal_len}"
                            );
                            cases += 1;
                            // Vary the last byte for the second output length.
                            if length != 0 {
                                data[length - 1] ^= 0xa5;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 4144);
    }
}
