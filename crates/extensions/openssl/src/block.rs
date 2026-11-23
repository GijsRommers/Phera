//! `openssl_encrypt($data, 'aes-256-cbc', $key, OPENSSL_RAW_DATA, $iv)`
//! uses PKCS#7 padding by default.

use aes::cipher::{
    BlockCipherDecrypt, BlockCipherEncrypt, BlockModeDecrypt, BlockModeEncrypt, BlockSizeUser,
    KeyInit, KeyIvInit, StreamCipher,
    block_padding::{NoPadding, Pkcs7},
    consts::U16,
};
use aes::{Aes128, Aes192, Aes256};

use crate::errors;
use crate::methods::{KeySize, Method, Mode};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Encrypt,
    Decrypt,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Padding {
    Pkcs7,
    None,
}

pub(crate) fn apply(
    method: Method,
    direction: Direction,
    padding: Padding,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Option<Vec<u8>> {
    match method.key {
        KeySize::Bits128 => aes::<Aes128>(method.mode, direction, padding, key, iv, data),
        KeySize::Bits192 => aes::<Aes192>(method.mode, direction, padding, key, iv, data),
        KeySize::Bits256 => aes::<Aes256>(method.mode, direction, padding, key, iv, data),
    }
}

pub(crate) fn failure(direction: Direction, data: &[u8]) -> &'static str {
    if direction == Direction::Decrypt && !data.is_empty() && data.len().is_multiple_of(16) {
        errors::BAD_DECRYPT
    } else {
        errors::WRONG_BLOCK_LENGTH
    }
}

fn aes<Cipher>(
    mode: Mode,
    direction: Direction,
    padding: Padding,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
) -> Option<Vec<u8>>
where
    Cipher: KeyInit + BlockSizeUser<BlockSize = U16> + BlockCipherEncrypt + BlockCipherDecrypt,
{
    match (mode, direction) {
        (Mode::Cbc, Direction::Encrypt) => encrypt(
            cbc::Encryptor::<Cipher>::new_from_slices(key, iv).ok()?,
            padding,
            data,
        ),
        (Mode::Cbc, Direction::Decrypt) => decrypt(
            cbc::Decryptor::<Cipher>::new_from_slices(key, iv).ok()?,
            padding,
            data,
        ),
        (Mode::Ecb, Direction::Encrypt) => encrypt(
            ecb::Encryptor::<Cipher>::new_from_slice(key).ok()?,
            padding,
            data,
        ),
        (Mode::Ecb, Direction::Decrypt) => decrypt(
            ecb::Decryptor::<Cipher>::new_from_slice(key).ok()?,
            padding,
            data,
        ),
        (Mode::Cfb, Direction::Encrypt) => {
            let mut result = data.to_vec();
            cfb_mode::Encryptor::<Cipher>::new_from_slices(key, iv)
                .ok()?
                .encrypt(&mut result);
            Some(result)
        }
        (Mode::Cfb, Direction::Decrypt) => {
            let mut result = data.to_vec();
            cfb_mode::Decryptor::<Cipher>::new_from_slices(key, iv)
                .ok()?
                .decrypt(&mut result);
            Some(result)
        }
        (Mode::Ctr, _) => stream(
            ctr::Ctr128BE::<Cipher>::new_from_slices(key, iv).ok()?,
            data,
        ),
        (Mode::Ofb, _) => stream(ofb::Ofb::<Cipher>::new_from_slices(key, iv).ok()?, data),
        _ => None,
    }
}

fn encrypt<Cipher: BlockModeEncrypt>(
    cipher: Cipher,
    padding: Padding,
    data: &[u8],
) -> Option<Vec<u8>> {
    let mut result = vec![0; data.len().checked_add(16)?];
    let output = match padding {
        Padding::Pkcs7 => cipher.encrypt_padded_b2b::<Pkcs7>(data, &mut result).ok()?,
        Padding::None => cipher
            .encrypt_padded_b2b::<NoPadding>(data, &mut result)
            .ok()?,
    };
    let length = output.len();
    result.truncate(length);
    Some(result)
}

fn decrypt<Cipher: BlockModeDecrypt>(
    cipher: Cipher,
    padding: Padding,
    data: &[u8],
) -> Option<Vec<u8>> {
    let mut result = vec![0; data.len()];
    let output = match padding {
        Padding::Pkcs7 => cipher.decrypt_padded_b2b::<Pkcs7>(data, &mut result).ok()?,
        Padding::None => cipher
            .decrypt_padded_b2b::<NoPadding>(data, &mut result)
            .ok()?,
    };
    let length = output.len();
    result.truncate(length);
    Some(result)
}

fn stream<Cipher: StreamCipher>(mut cipher: Cipher, data: &[u8]) -> Option<Vec<u8>> {
    let mut result = vec![0; data.len()];
    cipher.try_apply_keystream_b2b(data, &mut result).ok()?;
    Some(result)
}
