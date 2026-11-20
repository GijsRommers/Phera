//! Libsodium's secretstream wire format uses HChaCha20, ChaCha20 and Poly1305.

use blake2b_simd::Params;
use chacha20::{
    ChaChaCore, R20,
    cipher::{Block, KeyIvInit, StreamCipherCore},
    hchacha,
    variants::Ietf,
};
use poly1305::{
    Poly1305,
    universal_hash::{KeyInit, UniversalHash},
};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

pub(crate) const STATE_LEN: usize = 52;
pub(crate) const OVERHEAD: usize = 1 + 16;
pub(crate) const MAX_MESSAGE: u64 = (u32::MAX as u64 - 1) * 64;

type ChaCha20Core = ChaChaCore<R20, Ietf>;

pub(crate) struct Secretstream {
    key: [u8; 32],
    counter: u32,
    nonce: [u8; 8],
}

impl Secretstream {
    pub(crate) fn new(key: &[u8; 32], header: &[u8; 24]) -> Self {
        let prefix = header.first_chunk::<16>().expect("16-byte prefix fits");
        let nonce = header.last_chunk::<8>().expect("8-byte nonce fits");
        let key = Zeroizing::new(hchacha::<R20>(key.into(), prefix.into()).0);
        Self {
            key: *key,
            counter: 1,
            nonce: *nonce,
        }
    }

    pub(crate) fn encode(&self) -> [u8; STATE_LEN] {
        let mut output = [0; STATE_LEN];
        output[..32].copy_from_slice(&self.key);
        output[32..36].copy_from_slice(&self.counter.to_le_bytes());
        output[36..44].copy_from_slice(&self.nonce);
        let checksum = state_checksum(&output[..44]);
        output[44..].copy_from_slice(checksum.as_bytes());
        output
    }

    pub(crate) fn decode(input: &[u8; STATE_LEN]) -> Option<Self> {
        let (payload, checksum) = input.split_at(44);
        let counter = u32::from_le_bytes(payload[32..36].try_into().ok()?);
        if counter == 0 || !bool::from(state_checksum(payload).as_bytes().ct_eq(checksum)) {
            return None;
        }
        Some(Self {
            key: payload[..32].try_into().ok()?,
            counter,
            nonce: payload[36..].try_into().ok()?,
        })
    }

    /// The caller bounds the size so the payload's last block precedes counter wrap.
    pub(crate) fn push(&mut self, frame: &mut [u8], aad: &[u8], tag: u8) {
        let message_len = frame.len() - OVERHEAD;
        let (ciphertext, mac) = frame[1..].split_at_mut(message_len);
        let (mut stream, mut tag_block, poly) = self.frame_ciphers();

        tag_block[0] ^= tag;
        xor_payload(&mut stream, ciphertext);
        let digest = authenticate(poly, aad, &tag_block, ciphertext);
        mac.copy_from_slice(&digest);
        frame[0] = tag_block[0];
        self.advance(tag, &digest);
    }

    pub(crate) fn pull(&mut self, frame: &[u8], aad: &[u8]) -> Option<(Vec<u8>, u8)> {
        let message_len = frame.len().checked_sub(OVERHEAD)?;
        if message_len as u64 > MAX_MESSAGE {
            return None;
        }
        let (ciphertext, mac) = frame[1..].split_at(message_len);
        let (mut stream, mut tag_block, poly) = self.frame_ciphers();

        let tag = tag_block[0] ^ frame[0];
        tag_block[0] = frame[0];
        let digest = authenticate(poly, aad, &tag_block, ciphertext);
        if !bool::from(digest.as_slice().ct_eq(mac)) {
            return None;
        }
        let mut plaintext = ciphertext.to_vec();
        xor_payload(&mut stream, &mut plaintext);
        self.advance(tag, &digest);
        Some((plaintext, tag))
    }

    pub(crate) fn rekey(&mut self) {
        let mut stream = self.cipher();
        let mut new_material = Zeroizing::new([0; 40]);
        new_material[..32].copy_from_slice(&self.key);
        new_material[32..].copy_from_slice(&self.nonce);
        xor_payload(&mut stream, new_material.as_mut());
        self.key.copy_from_slice(&new_material[..32]);
        self.nonce.copy_from_slice(&new_material[32..]);
        self.counter = 1;
    }

    fn cipher(&self) -> ChaCha20Core {
        let mut nonce = [0; 12];
        nonce[..4].copy_from_slice(&self.counter.to_le_bytes());
        nonce[4..].copy_from_slice(&self.nonce);
        ChaCha20Core::new((&self.key).into(), (&nonce).into())
    }

    fn frame_ciphers(&self) -> (ChaCha20Core, Zeroizing<[u8; 64]>, Poly1305) {
        let mut stream = self.cipher();
        let mut block = Zeroizing::new([0; 64]);
        stream.write_keystream_block((&mut *block).into());
        let poly = Poly1305::new(block.first_chunk::<32>().expect("32-byte key fits").into());
        stream.write_keystream_block((&mut *block).into());
        (stream, block, poly)
    }

    fn advance(&mut self, tag: u8, digest: &poly1305::Tag) {
        const REKEY: u8 = 2;
        for (byte, mac) in self.nonce.iter_mut().zip(digest) {
            *byte ^= mac;
        }
        self.counter = self.counter.wrapping_add(1);
        if tag & REKEY != 0 || self.counter == 0 {
            self.rekey();
        }
    }
}

impl Drop for Secretstream {
    fn drop(&mut self) {
        self.key.zeroize();
        self.nonce.zeroize();
    }
}

fn xor_payload(stream: &mut ChaCha20Core, payload: &mut [u8]) {
    let (blocks, tail) = Block::<ChaCha20Core>::slice_as_chunks_mut(payload);
    stream.apply_keystream_blocks(blocks);
    if !tail.is_empty() {
        let mut block = Zeroizing::new([0; 64]);
        block[..tail.len()].copy_from_slice(tail);
        stream.apply_keystream_blocks(std::slice::from_mut((&mut *block).into()));
        tail.copy_from_slice(&block[..tail.len()]);
    }
}

fn authenticate(
    mut poly: Poly1305,
    aad: &[u8],
    tag_block: &[u8; 64],
    ciphertext: &[u8],
) -> poly1305::Tag {
    poly.update_padded(aad);
    poly.update(poly1305::Block::slice_as_chunks(tag_block).0);
    let (blocks, remainder) = poly1305::Block::slice_as_chunks(ciphertext);
    poly.update(blocks);

    // Libsodium's wire format appends ciphertext.len() % 16 zero bytes, then
    // the two 64-bit little-endian lengths. This can end in a partial block.
    let mut tail = [0; 48];
    tail[..remainder.len()].copy_from_slice(remainder);
    let offset = 2 * remainder.len();
    tail[offset..offset + 8].copy_from_slice(&(aad.len() as u64).to_le_bytes());
    tail[offset + 8..offset + 16].copy_from_slice(&(64 + ciphertext.len() as u64).to_le_bytes());
    poly.compute_unpadded(&tail[..offset + 16])
}

fn state_checksum(payload: &[u8]) -> blake2b_simd::Hash {
    Params::new()
        .hash_length(8)
        .personal(b"Phera SS state")
        .hash(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_wrap_rekeys_like_the_upstream_explicit_rekey_test() {
        let mut sender = Secretstream::new(&[0; 32], &[0; 24]);
        let mut receiver = Secretstream::new(&[0; 32], &[0; 24]);
        sender.counter = u32::MAX;
        receiver.counter = u32::MAX;
        let mut frame = [0; OVERHEAD];
        sender.push(&mut frame, b"", 0);
        assert_eq!(receiver.pull(&frame, b""), Some((vec![], 0)));
        assert_eq!(sender.counter, 1);
        assert_ne!(sender.key, Secretstream::new(&[0; 32], &[0; 24]).key);
        assert_eq!(sender.encode(), receiver.encode());
    }
}
