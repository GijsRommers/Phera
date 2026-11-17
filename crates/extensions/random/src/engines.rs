//! `new Random\Engine\PcgOneseq128XslRr64(42)` has a reproducible byte stream.
//! Ported from php-src/ext/random/engine_{pcgoneseq128xslrr64,xoshiro256starstar}.c.
use crate::mt19937::{Mode, Mt19937};
use phera_extension_api::NativeError;
const MULTIPLIER: u128 = (2549297995355413924u128 << 64) | 4865540595714422341;
const INCREMENT: u128 = (6364136223846793005u128 << 64) | 1442695040888963407;
#[derive(Clone)]
pub enum Engine {
    Mt(Box<Mt19937>),
    Pcg(u128),
    Xoshiro([u64; 4]),
    Secure,
}
impl Engine {
    pub fn pcg(seed: u128) -> Self {
        Self::Pcg(
            INCREMENT
                .wrapping_add(seed)
                .wrapping_mul(MULTIPLIER)
                .wrapping_add(INCREMENT),
        )
    }
    pub fn xoshiro(mut seed: u64) -> Self {
        let mut state = [0; 4];
        for value in &mut state {
            seed = seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut r = seed;
            r = (r ^ (r >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            r = (r ^ (r >> 27)).wrapping_mul(0x94d049bb133111eb);
            *value = r ^ (r >> 31);
        }
        Self::Xoshiro(state)
    }
    pub fn generate(&mut self) -> Result<(u64, usize), NativeError> {
        Ok(match self {
            Self::Mt(mt) => (u64::from(mt.next()), 4),
            Self::Pcg(state) => {
                *state = state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
                (
                    (((*state >> 64) as u64) ^ (*state as u64))
                        .rotate_right((*state >> 122) as u32),
                    8,
                )
            }
            Self::Xoshiro(s) => {
                let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
                let t = s[1] << 17;
                s[2] ^= s[0];
                s[3] ^= s[1];
                s[1] ^= s[2];
                s[0] ^= s[3];
                s[2] ^= t;
                s[3] = s[3].rotate_left(45);
                (result, 8)
            }
            Self::Secure => (secure_u64()?, 8),
        })
    }
    pub fn word64(&mut self) -> Result<u64, NativeError> {
        let (low, size) = self.generate()?;
        if size == 8 {
            Ok(low)
        } else {
            Ok(low | (self.generate()?.0 << 32))
        }
    }
    pub fn bounded(&mut self, max: u64, wide: bool) -> Result<u64, NativeError> {
        let mask = if wide { u64::MAX } else { u32::MAX as u64 };
        let size = max.wrapping_add(1);
        for _ in 0..=50 {
            let value = if wide {
                self.word64()?
            } else {
                self.generate()?.0 & mask
            };
            if max == mask {
                return Ok(value);
            }
            if size.is_power_of_two() {
                return Ok(value & max);
            }
            let limit = mask - (mask % size) - 1;
            if value <= limit {
                return Ok(value % size);
            }
        }
        Err(NativeError::throw(
            "Random\\BrokenRandomEngineError",
            "Failed to generate an acceptable random number in 50 attempts",
        ))
    }
    pub fn range(&mut self, min: i64, max: i64) -> Result<i64, NativeError> {
        let width = (max as u64).wrapping_sub(min as u64);
        Ok(self
            .bounded(width, width > u32::MAX as u64)?
            .wrapping_add(min as u64) as i64)
    }
    pub fn integer(&mut self, min: i64, max: i64) -> Result<i64, NativeError> {
        if let Self::Mt(mt) = self
            && mt.mode == Mode::Legacy
        {
            let r = mt.next() >> 1;
            let offset = ((max as f64 - min as f64 + 1.0) * (r as f64 / 2147483648.0)) as u64;
            return Ok(offset.wrapping_add(min as u64) as i64);
        }
        self.range(min, max)
    }
    pub fn jump(&mut self, mut advance: u64) {
        if let Self::Pcg(state) = self {
            let (mut multiplier, mut add) = (MULTIPLIER, INCREMENT);
            let (mut acc_multiplier, mut acc_add) = (1u128, 0u128);
            while advance > 0 {
                if advance & 1 != 0 {
                    acc_multiplier = acc_multiplier.wrapping_mul(multiplier);
                    acc_add = acc_add.wrapping_mul(multiplier).wrapping_add(add);
                }
                add = multiplier.wrapping_add(1).wrapping_mul(add);
                multiplier = multiplier.wrapping_mul(multiplier);
                advance >>= 1;
            }
            *state = acc_multiplier.wrapping_mul(*state).wrapping_add(acc_add);
        }
    }
    pub fn jump_xoshiro(&mut self, long: bool) -> Result<(), NativeError> {
        let constants = if long {
            [
                0x76e15d3efefdcbbf,
                0xc5004e441c522fb3,
                0x77710069854ee241,
                0x39109bb02acbe635,
            ]
        } else {
            [
                0x180ec6d33cfd0aba,
                0xd5a61266f0c9392c,
                0xa9582618e03fc9aa,
                0x39abdc4529b1661c,
            ]
        };
        let mut result = [0; 4];
        for constant in constants {
            for bit in 0..64 {
                if constant & (1u64 << bit) != 0
                    && let Self::Xoshiro(state) = self
                {
                    for (a, b) in result.iter_mut().zip(state) {
                        *a ^= *b;
                    }
                }
                self.generate()?;
            }
        }
        *self = Self::Xoshiro(result);
        Ok(())
    }
}
pub fn secure_u64() -> Result<u64, NativeError> {
    let mut bytes = [0; 8];
    fill(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}
pub fn fill(bytes: &mut [u8]) -> Result<(), NativeError> {
    getrandom::fill(bytes).map_err(|e| NativeError::throw("Random\\RandomException", e.to_string()))
}
