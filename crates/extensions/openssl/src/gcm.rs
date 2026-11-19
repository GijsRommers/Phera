//! `openssl_encrypt($data, 'aes-256-gcm', $key, 1, $iv, $tag)` accepts
//! variable IV lengths. RustCrypto selects the nonce length at compile time.

use aes::cipher::{BlockCipherEncClosure, BlockCipherEncrypt, BlockSizeUser, KeyInit, consts::U16};
use aes::{Aes128, Aes192, Aes256};
use aes_gcm::AesGcm;
use aes_gcm::aead::array::{ArraySize, typenum::U};
use aes_gcm::aead::consts::{U4, U8, U12, U13, U14, U15};

use crate::aead::{self, Failure, Input, Operation, Output};
use crate::methods::KeySize;

type Result = std::result::Result<Output, Failure>;

pub(crate) fn apply(key: KeySize, input: Input<'_>) -> Result {
    if input.nonce.len() == 12 {
        // Keep a concrete AES type for the 96-bit GCM fast path.
        return match key {
            KeySize::Bits128 => gcm::<_, U12>(cipher::<Aes128>(input)?, input),
            KeySize::Bits192 => gcm::<_, U12>(cipher::<Aes192>(input)?, input),
            KeySize::Bits256 => gcm::<_, U12>(cipher::<Aes256>(input)?, input),
        };
    }
    let aes = match key {
        KeySize::Bits128 => Aes::Bits128(cipher::<Aes128>(input)?),
        KeySize::Bits192 => Aes::Bits192(cipher::<Aes192>(input)?),
        KeySize::Bits256 => Aes::Bits256(cipher::<Aes256>(input)?),
    };
    variable_nonce(aes, input)
}

fn cipher<Cipher: KeyInit>(input: Input<'_>) -> std::result::Result<Cipher, Failure> {
    Cipher::new_from_slice(input.key).map_err(|_| Failure::Crypto)
}

// Forward the encrypt-only trait to the AES implementation for the selected
// key length. This shares the variable nonce dispatch across all three keys.
enum Aes {
    Bits128(Aes128),
    Bits192(Aes192),
    Bits256(Aes256),
}

impl BlockSizeUser for Aes {
    type BlockSize = U16;
}

impl BlockCipherEncrypt for Aes {
    fn encrypt_with_backend(&self, operation: impl BlockCipherEncClosure<BlockSize = U16>) {
        match self {
            Self::Bits128(aes) => aes.encrypt_with_backend(operation),
            Self::Bits192(aes) => aes.encrypt_with_backend(operation),
            Self::Bits256(aes) => aes.encrypt_with_backend(operation),
        }
    }
}

fn gcm<Cipher, Size>(aes: Cipher, input: Input<'_>) -> Result
where
    Cipher: BlockCipherEncrypt + BlockSizeUser<BlockSize = U16>,
    Size: ArraySize,
{
    let Operation::Decrypt(tag) = input.operation else {
        return aead::with_cipher(&AesGcm::<Cipher, Size>::from(aes), input);
    };
    match tag.len() {
        4 => aead::with_cipher(&AesGcm::<Cipher, Size, U4>::from(aes), input),
        8 => aead::with_cipher(&AesGcm::<Cipher, Size, U8>::from(aes), input),
        12 => aead::with_cipher(&AesGcm::<Cipher, Size, U12>::from(aes), input),
        13 => aead::with_cipher(&AesGcm::<Cipher, Size, U13>::from(aes), input),
        14 => aead::with_cipher(&AesGcm::<Cipher, Size, U14>::from(aes), input),
        15 => aead::with_cipher(&AesGcm::<Cipher, Size, U15>::from(aes), input),
        16 => aead::with_cipher(&AesGcm::<Cipher, Size>::from(aes), input),
        _ => Err(Failure::UnsupportedTag),
    }
}

fn variable_nonce(aes: Aes, input: Input<'_>) -> Result {
    match input.nonce.len() {
        1 => gcm::<_, U<1>>(aes, input),
        2 => gcm::<_, U<2>>(aes, input),
        3 => gcm::<_, U<3>>(aes, input),
        4 => gcm::<_, U<4>>(aes, input),
        5 => gcm::<_, U<5>>(aes, input),
        6 => gcm::<_, U<6>>(aes, input),
        7 => gcm::<_, U<7>>(aes, input),
        8 => gcm::<_, U<8>>(aes, input),
        9 => gcm::<_, U<9>>(aes, input),
        10 => gcm::<_, U<10>>(aes, input),
        11 => gcm::<_, U<11>>(aes, input),
        13 => gcm::<_, U<13>>(aes, input),
        14 => gcm::<_, U<14>>(aes, input),
        15 => gcm::<_, U<15>>(aes, input),
        16 => gcm::<_, U<16>>(aes, input),
        17 => gcm::<_, U<17>>(aes, input),
        18 => gcm::<_, U<18>>(aes, input),
        19 => gcm::<_, U<19>>(aes, input),
        20 => gcm::<_, U<20>>(aes, input),
        21 => gcm::<_, U<21>>(aes, input),
        22 => gcm::<_, U<22>>(aes, input),
        23 => gcm::<_, U<23>>(aes, input),
        24 => gcm::<_, U<24>>(aes, input),
        25 => gcm::<_, U<25>>(aes, input),
        26 => gcm::<_, U<26>>(aes, input),
        27 => gcm::<_, U<27>>(aes, input),
        28 => gcm::<_, U<28>>(aes, input),
        29 => gcm::<_, U<29>>(aes, input),
        30 => gcm::<_, U<30>>(aes, input),
        31 => gcm::<_, U<31>>(aes, input),
        32 => gcm::<_, U<32>>(aes, input),
        33 => gcm::<_, U<33>>(aes, input),
        34 => gcm::<_, U<34>>(aes, input),
        35 => gcm::<_, U<35>>(aes, input),
        36 => gcm::<_, U<36>>(aes, input),
        37 => gcm::<_, U<37>>(aes, input),
        38 => gcm::<_, U<38>>(aes, input),
        39 => gcm::<_, U<39>>(aes, input),
        40 => gcm::<_, U<40>>(aes, input),
        41 => gcm::<_, U<41>>(aes, input),
        42 => gcm::<_, U<42>>(aes, input),
        43 => gcm::<_, U<43>>(aes, input),
        44 => gcm::<_, U<44>>(aes, input),
        45 => gcm::<_, U<45>>(aes, input),
        46 => gcm::<_, U<46>>(aes, input),
        47 => gcm::<_, U<47>>(aes, input),
        48 => gcm::<_, U<48>>(aes, input),
        49 => gcm::<_, U<49>>(aes, input),
        50 => gcm::<_, U<50>>(aes, input),
        51 => gcm::<_, U<51>>(aes, input),
        52 => gcm::<_, U<52>>(aes, input),
        53 => gcm::<_, U<53>>(aes, input),
        54 => gcm::<_, U<54>>(aes, input),
        55 => gcm::<_, U<55>>(aes, input),
        56 => gcm::<_, U<56>>(aes, input),
        57 => gcm::<_, U<57>>(aes, input),
        58 => gcm::<_, U<58>>(aes, input),
        59 => gcm::<_, U<59>>(aes, input),
        60 => gcm::<_, U<60>>(aes, input),
        61 => gcm::<_, U<61>>(aes, input),
        62 => gcm::<_, U<62>>(aes, input),
        63 => gcm::<_, U<63>>(aes, input),
        64 => gcm::<_, U<64>>(aes, input),
        65 => gcm::<_, U<65>>(aes, input),
        66 => gcm::<_, U<66>>(aes, input),
        67 => gcm::<_, U<67>>(aes, input),
        68 => gcm::<_, U<68>>(aes, input),
        69 => gcm::<_, U<69>>(aes, input),
        70 => gcm::<_, U<70>>(aes, input),
        71 => gcm::<_, U<71>>(aes, input),
        72 => gcm::<_, U<72>>(aes, input),
        73 => gcm::<_, U<73>>(aes, input),
        74 => gcm::<_, U<74>>(aes, input),
        75 => gcm::<_, U<75>>(aes, input),
        76 => gcm::<_, U<76>>(aes, input),
        77 => gcm::<_, U<77>>(aes, input),
        78 => gcm::<_, U<78>>(aes, input),
        79 => gcm::<_, U<79>>(aes, input),
        80 => gcm::<_, U<80>>(aes, input),
        81 => gcm::<_, U<81>>(aes, input),
        82 => gcm::<_, U<82>>(aes, input),
        83 => gcm::<_, U<83>>(aes, input),
        84 => gcm::<_, U<84>>(aes, input),
        85 => gcm::<_, U<85>>(aes, input),
        86 => gcm::<_, U<86>>(aes, input),
        87 => gcm::<_, U<87>>(aes, input),
        88 => gcm::<_, U<88>>(aes, input),
        89 => gcm::<_, U<89>>(aes, input),
        90 => gcm::<_, U<90>>(aes, input),
        91 => gcm::<_, U<91>>(aes, input),
        92 => gcm::<_, U<92>>(aes, input),
        93 => gcm::<_, U<93>>(aes, input),
        94 => gcm::<_, U<94>>(aes, input),
        95 => gcm::<_, U<95>>(aes, input),
        96 => gcm::<_, U<96>>(aes, input),
        97 => gcm::<_, U<97>>(aes, input),
        98 => gcm::<_, U<98>>(aes, input),
        99 => gcm::<_, U<99>>(aes, input),
        100 => gcm::<_, U<100>>(aes, input),
        101 => gcm::<_, U<101>>(aes, input),
        102 => gcm::<_, U<102>>(aes, input),
        103 => gcm::<_, U<103>>(aes, input),
        104 => gcm::<_, U<104>>(aes, input),
        105 => gcm::<_, U<105>>(aes, input),
        106 => gcm::<_, U<106>>(aes, input),
        107 => gcm::<_, U<107>>(aes, input),
        108 => gcm::<_, U<108>>(aes, input),
        109 => gcm::<_, U<109>>(aes, input),
        110 => gcm::<_, U<110>>(aes, input),
        111 => gcm::<_, U<111>>(aes, input),
        112 => gcm::<_, U<112>>(aes, input),
        113 => gcm::<_, U<113>>(aes, input),
        114 => gcm::<_, U<114>>(aes, input),
        115 => gcm::<_, U<115>>(aes, input),
        116 => gcm::<_, U<116>>(aes, input),
        117 => gcm::<_, U<117>>(aes, input),
        118 => gcm::<_, U<118>>(aes, input),
        119 => gcm::<_, U<119>>(aes, input),
        120 => gcm::<_, U<120>>(aes, input),
        121 => gcm::<_, U<121>>(aes, input),
        122 => gcm::<_, U<122>>(aes, input),
        123 => gcm::<_, U<123>>(aes, input),
        124 => gcm::<_, U<124>>(aes, input),
        125 => gcm::<_, U<125>>(aes, input),
        126 => gcm::<_, U<126>>(aes, input),
        127 => gcm::<_, U<127>>(aes, input),
        128 => gcm::<_, U<128>>(aes, input),
        _ => Err(Failure::Crypto),
    }
}
