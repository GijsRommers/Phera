//! `openssl_encrypt($data, 'aes-256-gcm', $key, 1, $iv, $tag, $aad)`
//! returns ciphertext separately from its authentication tag.

use aes::cipher::{BlockCipherEncrypt, BlockSizeUser, KeyInit, consts::U16};
use aes::{Aes128, Aes192, Aes256};
use aes_gcm::AesGcm;
use aes_gcm::aead::{AeadInOut, Nonce, Tag, array::ArraySize, consts::*};
use ccm::{Ccm, NonceSize as CcmNonce, TagSize as CcmTag};
use chacha20poly1305::ChaCha20Poly1305;

use crate::methods::{KeySize, Method, Mode};

#[derive(Clone, Copy)]
pub(crate) enum Operation<'a> {
    Encrypt(usize),
    Decrypt(&'a [u8]),
}

impl Operation<'_> {
    fn tag_length(self) -> usize {
        match self {
            Self::Encrypt(length) => length,
            Self::Decrypt(tag) => tag.len(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Input<'a> {
    pub(crate) key: &'a [u8],
    pub(crate) nonce: &'a [u8],
    pub(crate) data: &'a [u8],
    pub(crate) aad: &'a [u8],
    pub(crate) operation: Operation<'a>,
}

pub(crate) struct Output {
    pub(crate) data: Vec<u8>,
    pub(crate) tag: Option<Vec<u8>>,
}

#[derive(Debug)]
pub(crate) enum Failure {
    Crypto,
    UnsupportedNonce,
    UnsupportedTag,
}

type Result = std::result::Result<Output, Failure>;

pub(crate) fn apply(method: Method, input: Input<'_>) -> Result {
    if method.mode == Mode::ChaCha {
        return run::<ChaCha20Poly1305>(input);
    }
    match method.key {
        KeySize::Bits128 => aes::<Aes128>(method.mode, input),
        KeySize::Bits192 => aes::<Aes192>(method.mode, input),
        KeySize::Bits256 => aes::<Aes256>(method.mode, input),
    }
}

fn aes<Cipher>(mode: Mode, input: Input<'_>) -> Result
where
    Cipher: BlockCipherEncrypt + BlockSizeUser<BlockSize = U16> + KeyInit,
{
    match mode {
        Mode::Gcm => match input.nonce.len() {
            1 => gcm::<Cipher, U1>(input),
            7 => gcm::<Cipher, U7>(input),
            8 => gcm::<Cipher, U8>(input),
            11 => gcm::<Cipher, U11>(input),
            12 => gcm::<Cipher, U12>(input),
            13 => gcm::<Cipher, U13>(input),
            16 => gcm::<Cipher, U16>(input),
            24 => gcm::<Cipher, U24>(input),
            32 => gcm::<Cipher, U32>(input),
            64 => gcm::<Cipher, U64>(input),
            128 => gcm::<Cipher, U128>(input),
            _ => Err(Failure::UnsupportedNonce),
        },
        Mode::Ccm => match input.nonce.len() {
            7 => ccm::<Cipher, U7>(input),
            8 => ccm::<Cipher, U8>(input),
            9 => ccm::<Cipher, U9>(input),
            10 => ccm::<Cipher, U10>(input),
            11 => ccm::<Cipher, U11>(input),
            12 => ccm::<Cipher, U12>(input),
            13 => ccm::<Cipher, U13>(input),
            _ => Err(Failure::Crypto),
        },
        _ => Err(Failure::Crypto),
    }
}

fn gcm<Cipher, Size>(input: Input<'_>) -> Result
where
    Cipher: BlockCipherEncrypt + BlockSizeUser<BlockSize = U16> + KeyInit,
    Size: ArraySize,
{
    let Operation::Decrypt(tag) = input.operation else {
        return run::<AesGcm<Cipher, Size>>(input);
    };
    match tag.len() {
        4 => run::<AesGcm<Cipher, Size, U4>>(input),
        8 => run::<AesGcm<Cipher, Size, U8>>(input),
        12 => run::<AesGcm<Cipher, Size, U12>>(input),
        13 => run::<AesGcm<Cipher, Size, U13>>(input),
        14 => run::<AesGcm<Cipher, Size, U14>>(input),
        15 => run::<AesGcm<Cipher, Size, U15>>(input),
        16 => run::<AesGcm<Cipher, Size>>(input),
        _ => Err(Failure::UnsupportedTag),
    }
}

fn ccm<Cipher, Size>(input: Input<'_>) -> Result
where
    Cipher: BlockCipherEncrypt + BlockSizeUser<BlockSize = U16> + KeyInit,
    Size: CcmNonce + ArraySize,
{
    match input.operation.tag_length() {
        4 => ccm_with_tag::<Cipher, U4, Size>(input),
        6 => ccm_with_tag::<Cipher, U6, Size>(input),
        8 => ccm_with_tag::<Cipher, U8, Size>(input),
        10 => ccm_with_tag::<Cipher, U10, Size>(input),
        12 => ccm_with_tag::<Cipher, U12, Size>(input),
        14 => ccm_with_tag::<Cipher, U14, Size>(input),
        16 => ccm_with_tag::<Cipher, U16, Size>(input),
        _ => Err(Failure::Crypto),
    }
}

fn ccm_with_tag<Cipher, AuthTag, Size>(input: Input<'_>) -> Result
where
    Cipher: BlockCipherEncrypt + BlockSizeUser<BlockSize = U16> + KeyInit,
    AuthTag: CcmTag + ArraySize,
    Size: CcmNonce + ArraySize,
{
    run::<Ccm<Cipher, AuthTag, Size>>(input)
}

fn run<Cipher: AeadInOut + KeyInit>(input: Input<'_>) -> Result {
    let cipher = Cipher::new_from_slice(input.key).map_err(|_| Failure::Crypto)?;
    let nonce = <&Nonce<Cipher>>::try_from(input.nonce).map_err(|_| Failure::Crypto)?;
    let mut data = vec![0; input.data.len()];
    let buffer =
        aes_gcm::aead::inout::InOutBuf::new(input.data, &mut data).map_err(|_| Failure::Crypto)?;
    match input.operation {
        Operation::Encrypt(length) => {
            let tag = cipher
                .encrypt_inout_detached(nonce, input.aad, buffer)
                .map_err(|_| Failure::Crypto)?;
            let tag = tag.get(..length).ok_or(Failure::Crypto)?.to_vec();
            Ok(Output {
                data,
                tag: Some(tag),
            })
        }
        Operation::Decrypt(tag) => {
            let tag = <&Tag<Cipher>>::try_from(tag).map_err(|_| Failure::UnsupportedTag)?;
            cipher
                .decrypt_inout_detached(nonce, input.aad, buffer, tag)
                .map_err(|_| Failure::Crypto)?;
            Ok(Output { data, tag: None })
        }
    }
}
