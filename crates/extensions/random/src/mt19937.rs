//! The Mersenne Twister behind `mt_rand()`, `rand()`, the shuffles and
//! `Random\Engine\Mt19937`, including PHP's old, wrong variant:
//!
//! ```php
//! mt_srand(42);
//! mt_rand();                    // the same number in every PHP since 7.1
//! mt_srand(42, MT_RAND_PHP);    // Deprecated: the variant from before 7.1
//! ```
//!
//! Ported from php-src/ext/random/engine_mt19937.c; see LICENSE-PHP and LICENSE-MT19937.

/// `MT_RAND_MT19937`
pub const MT_RAND_MT19937: i64 = 0;
/// `MT_RAND_PHP`
pub const MT_RAND_PHP: i64 = 1;

/// The state is 624 words of 32 bits.
const N: usize = 624;
/// The twist combines each word with the one this far ahead.
const M: usize = 397;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `MT_RAND_MT19937`, the real algorithm.
    Standard,
    /// `MT_RAND_PHP`: before PHP 7.1 the twist took the low bit of the
    /// wrong word, and ranges were scaled instead of drawn uniformly.
    Legacy,
}

impl Mode {
    /// The `$mode` argument; `None` for a value that is neither constant.
    pub fn from_flag(flag: i64) -> Option<Mode> {
        match flag {
            MT_RAND_MT19937 => Some(Mode::Standard),
            MT_RAND_PHP => Some(Mode::Legacy),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Mt19937 {
    state: [u32; N],
    /// The next word to temper; `N` when the state needs a twist.
    index: usize,
    pub mode: Mode,
}

impl Mt19937 {
    pub fn new(seed: u32, mode: Mode) -> Self {
        let mut state = [0; N];
        state[0] = seed;
        for i in 1..N {
            let previous = state[i - 1];
            state[i] = 1812433253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Self {
            state,
            index: N,
            mode,
        }
    }

    pub fn next(&mut self) -> u32 {
        if self.index == N {
            self.twist();
        }
        let mut value = self.state[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c5680;
        value ^= (value << 15) & 0xefc60000;
        value ^ (value >> 18)
    }

    fn twist(&mut self) {
        for i in 0..N {
            let word = self.state[i];
            let next = self.state[(i + 1) % N];
            let low_bit = match self.mode {
                Mode::Standard => next & 1,
                Mode::Legacy => word & 1,
            };
            // Mix in the matrix when the low bit is set, without a branch.
            self.state[i] = self.state[(i + M) % N]
                ^ (((word & 0x80000000) | (next & 0x7fffffff)) >> 1)
                ^ (0x9908b0df & 0u32.wrapping_sub(low_bit));
        }
        self.index = 0;
    }
}
