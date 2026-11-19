//! `sodium_crypto_generichash_update($state, $chunk)` retains only one BLAKE2b block.

use blake2b_simd::Params;
use revm_precompile::blake2::compress;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

pub(crate) const STATE_LEN: usize = 384;
const MAGIC: &[u8; 8] = b"PHERAGH1";
const BLOCK_LEN: usize = 128;
const CHECKSUM_OFFSET: usize = STATE_LEN - 32;

const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

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
            compress_block(&mut self.words, &self.pending, self.compressed, false);
            self.pending.zeroize();
            self.used = 0;
            input = tail;
            // BLAKE2b needs even a full last block until the final flag is known.
            while input.len() > BLOCK_LEN {
                let (block, tail) = input.split_first_chunk::<BLOCK_LEN>()?;
                self.compressed = self.compressed.checked_add(BLOCK_LEN as u128)?;
                compress_block(&mut self.words, block, self.compressed, false);
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
        compress_block(&mut self.words, &self.pending, total, true);
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

fn compress_block(words: &mut [u64; 8], block: &[u8; BLOCK_LEN], length: u128, final_block: bool) {
    let mut message = [0; 16];
    for (word, chunk) in message.iter_mut().zip(block.as_chunks::<8>().0) {
        *word = u64::from_le_bytes(*chunk);
    }
    let count = [length as u64, (length >> 64) as u64];
    compress(12, words, &message, &count, final_block);
    message.zeroize();
}

fn checksum(payload: &[u8]) -> blake2b_simd::Hash {
    Params::new()
        .hash_length(32)
        .personal(b"Phera GH state")
        .hash(payload)
}
