//! The original `sodium_crypto_aead_chacha20poly1305_encrypt(...)` takes an eight-byte nonce.

use chacha20::{
    ChaCha20Legacy,
    cipher::{KeyIvInit, StreamCipher, StreamCipherSeek},
};
use poly1305::{Poly1305, universal_hash::KeyInit};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::aead::Operation;

pub(super) fn crypt(
    operation: Operation,
    key: &[u8],
    nonce: &[u8],
    input: &[u8],
    aad: &[u8],
) -> Option<Vec<u8>> {
    let mut cipher = ChaCha20Legacy::new_from_slices(key, nonce).ok()?;
    let mut mac_key = Zeroizing::new([0_u8; 32]);
    cipher.try_apply_keystream(mac_key.as_mut()).ok()?;
    let mac = Poly1305::new((&*mac_key).into());
    cipher.try_seek(64_u64).ok()?;

    match operation {
        Operation::Encrypt => {
            let mut output = input.to_vec();
            cipher.try_apply_keystream(&mut output).ok()?;
            let tag = authenticate(mac, aad, &output);
            output.extend_from_slice(&tag);
            Some(output)
        }
        Operation::Decrypt => {
            let (ciphertext, tag) = input.split_at(input.len().checked_sub(16)?);
            if !bool::from(authenticate(mac, aad, ciphertext).as_slice().ct_eq(tag)) {
                return None;
            }
            let mut output = ciphertext.to_vec();
            cipher.try_apply_keystream(&mut output).ok()?;
            Some(output)
        }
    }
}

fn authenticate(mac: Poly1305, aad: &[u8], ciphertext: &[u8]) -> poly1305::Tag {
    // The original construction authenticates each field immediately followed by its
    // 64-bit little-endian length. The IETF layout is handled by chacha20poly1305.
    let transcript = [
        aad,
        &(aad.len() as u64).to_le_bytes(),
        ciphertext,
        &(ciphertext.len() as u64).to_le_bytes(),
    ]
    .concat();
    mac.compute_unpadded(&transcript)
}
