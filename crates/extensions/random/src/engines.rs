//! The four engines and the ranges drawn from them. A seeded engine gives
//! the same numbers as PHP's, draw for draw:
//!
//! ```php
//! $engine = new Random\Engine\PcgOneseq128XslRr64(42);
//! bin2hex($engine->generate());           // the same 8 bytes in every PHP
//! (new Random\Randomizer($engine))->getInt(1, 6);
//! ```
//!
//! Ported from php-src/ext/random/engine_{pcgoneseq128xslrr64,xoshiro256starstar}.c.
//! The ranges follow php-src/ext/random/random.c.

use phera_extension_api::NativeError;

use crate::mt19937::{Mode, Mt19937};

/// PCG's 128-bit multiplier and increment.
const PCG_MULTIPLIER: u128 = (2549297995355413924u128 << 64) | 4865540595714422341;
const PCG_INCREMENT: u128 = (6364136223846793005u128 << 64) | 1442695040888963407;

/// Xoshiro256**'s jump polynomials: `jump()` advances 2^128 draws,
/// `jumpLong()` 2^192.
pub const XOSHIRO_JUMP: [u64; 4] = [
    0x180ec6d33cfd0aba,
    0xd5a61266f0c9392c,
    0xa9582618e03fc9aa,
    0x39abdc4529b1661c,
];
pub const XOSHIRO_LONG_JUMP: [u64; 4] = [
    0x76e15d3efefdcbbf,
    0xc5004e441c522fb3,
    0x77710069854ee241,
    0x39109bb02acbe635,
];

/// How often a range draws again before PHP blames the engine.
pub const ATTEMPTS: usize = 50;

#[derive(Clone)]
pub enum Engine {
    /// Boxed: its 2.5 KB of state would make every engine that size.
    Mt(Box<Mt19937>),
    Pcg(u128),
    Xoshiro([u64; 4]),
    /// The operating system's generator, which has no state.
    Secure,
}

impl Engine {
    pub fn pcg(seed: u128) -> Self {
        let state = PCG_INCREMENT.wrapping_add(seed);
        Self::Pcg(
            state
                .wrapping_mul(PCG_MULTIPLIER)
                .wrapping_add(PCG_INCREMENT),
        )
    }

    /// Seeds the four words from one with SplitMix64, as PHP does.
    pub fn xoshiro(mut seed: u64) -> Self {
        let mut state = [0; 4];
        for word in &mut state {
            seed = seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            *word = z ^ (z >> 31);
        }
        Self::Xoshiro(state)
    }

    /// One draw, and how many of its bytes are random: 4 for Mt19937, 8
    /// for the others.
    pub fn generate(&mut self) -> Result<(u64, usize), NativeError> {
        Ok(match self {
            Self::Mt(mt) => (u64::from(mt.next()), 4),
            Self::Pcg(state) => {
                *state = state
                    .wrapping_mul(PCG_MULTIPLIER)
                    .wrapping_add(PCG_INCREMENT);
                // XSL RR: fold the halves, rotate by the top six bits.
                let folded = ((*state >> 64) as u64) ^ (*state as u64);
                (folded.rotate_right((*state >> 122) as u32), 8)
            }
            Self::Xoshiro(state) => (xoshiro_next(state), 8),
            Self::Secure => (secure_u64()?, 8),
        })
    }

    /// 64 random bits: one draw, or two of Mt19937's, the first low.
    pub fn word64(&mut self) -> Result<u64, NativeError> {
        let (low, size) = self.generate()?;
        if size == 8 {
            return Ok(low);
        }
        Ok(low | (self.generate()?.0 << 32))
    }

    /// A uniform number in `0..=max` from 32 random bits, as php-src's
    /// `php_random_range32`.
    pub fn range32(&mut self, max: u32) -> Result<u32, NativeError> {
        // Only the low 32 bits of a draw are used.
        let mut value = self.generate()?.0 as u32;
        if max == u32::MAX {
            return Ok(value);
        }
        let size = max + 1;
        if size.is_power_of_two() {
            return Ok(value & max);
        }
        // Values above the last whole multiple of `size` would favour the
        // small results, so they are drawn again.
        let limit = u32::MAX - (u32::MAX % size) - 1;
        let mut attempts = 0;
        while value > limit {
            attempts += 1;
            if attempts > ATTEMPTS {
                return Err(broken_engine());
            }
            value = self.generate()?.0 as u32;
        }
        Ok(value % size)
    }

    /// A uniform number in `0..=max` from 64 random bits, as php-src's
    /// `php_random_range64`.
    pub fn range64(&mut self, max: u64) -> Result<u64, NativeError> {
        let mut value = self.word64()?;
        if max == u64::MAX {
            return Ok(value);
        }
        let size = max + 1;
        if size.is_power_of_two() {
            return Ok(value & max);
        }
        let limit = u64::MAX - (u64::MAX % size) - 1;
        let mut attempts = 0;
        while value > limit {
            attempts += 1;
            if attempts > ATTEMPTS {
                return Err(broken_engine());
            }
            value = self.word64()?;
        }
        Ok(value % size)
    }

    /// A uniform number in `min..=max`, from 32 bits when the range fits.
    pub fn range(&mut self, min: i64, max: i64) -> Result<i64, NativeError> {
        let width = (max as u64).wrapping_sub(min as u64);
        let offset = match u32::try_from(width) {
            Ok(width) => u64::from(self.range32(width)?),
            Err(_) => self.range64(width)?,
        };
        Ok(offset.wrapping_add(min as u64) as i64)
    }

    /// What `mt_rand($min, $max)` and `getInt()` draw: a range, except
    /// that Mt19937 in its legacy mode scales one draw as PHP 5 did, which
    /// is biased.
    pub fn integer(&mut self, min: i64, max: i64) -> Result<i64, NativeError> {
        if let Self::Mt(mt) = self
            && mt.mode == Mode::Legacy
        {
            let draw = f64::from(mt.next() >> 1);
            let scale = (max as f64 - min as f64 + 1.0) * (draw / 2147483648.0);
            return Ok((scale as u64).wrapping_add(min as u64) as i64);
        }
        self.range(min, max)
    }
}

/// Advances a PCG state by `advance` draws in O(log advance) steps.
pub fn pcg_jump(state: &mut u128, mut advance: u64) {
    let (mut multiplier, mut increment) = (PCG_MULTIPLIER, PCG_INCREMENT);
    let (mut total_multiplier, mut total_increment) = (1u128, 0u128);
    while advance > 0 {
        if advance & 1 != 0 {
            total_multiplier = total_multiplier.wrapping_mul(multiplier);
            total_increment = total_increment
                .wrapping_mul(multiplier)
                .wrapping_add(increment);
        }
        increment = multiplier.wrapping_add(1).wrapping_mul(increment);
        multiplier = multiplier.wrapping_mul(multiplier);
        advance >>= 1;
    }
    *state = total_multiplier
        .wrapping_mul(*state)
        .wrapping_add(total_increment);
}

/// One Xoshiro256** draw.
fn xoshiro_next(s: &mut [u64; 4]) -> u64 {
    let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let t = s[1] << 17;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = s[3].rotate_left(45);
    result
}

/// Advances a Xoshiro256** state by a jump `polynomial`: the result is the
/// XOR of the states at the polynomial's set bits.
pub fn xoshiro_jump(state: &mut [u64; 4], polynomial: [u64; 4]) {
    let mut jumped = [0; 4];
    for word in polynomial {
        for bit in 0..64 {
            if word & (1 << bit) != 0 {
                for (sum, value) in jumped.iter_mut().zip(*state) {
                    *sum ^= value;
                }
            }
            xoshiro_next(state);
        }
    }
    *state = jumped;
}

pub fn broken_engine() -> NativeError {
    NativeError::throw(
        "Random\\BrokenRandomEngineError",
        format!("Failed to generate an acceptable random number in {ATTEMPTS} attempts"),
    )
}

/// 64 bits from the operating system.
pub fn secure_u64() -> Result<u64, NativeError> {
    let mut bytes = [0; 8];
    fill(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Fills `bytes` from the operating system.
pub fn fill(bytes: &mut [u8]) -> Result<(), NativeError> {
    getrandom::fill(bytes)
        .map_err(|error| NativeError::throw("Random\\RandomException", error.to_string()))
}
