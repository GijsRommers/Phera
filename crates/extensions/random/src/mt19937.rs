//! The Mersenne Twister behind `mt_rand()`, `rand()`, the shuffles and
//! `Random\Engine\Mt19937`:
//!
//! ```php
//! mt_srand(42);
//! mt_rand();                    // the same number in every PHP since 7.1
//! ```
//!
//! Ported from php-src/ext/random/engine_mt19937.c; see LICENSE-PHP and LICENSE-MT19937.

/// `MT_RAND_MT19937`
pub const MT_RAND_MT19937: i64 = 0;

/// PHP's numeric selector for the deprecated variant is 1, even when the
/// `MT_RAND_PHP` constant is absent. Reject it before changing any state.
pub fn reject_legacy_mode(mode: i64) -> Result<(), phera_extension_api::NativeError> {
    if mode != 1 {
        return Ok(());
    }
    use phera_source::deprecations::{ItemKind, omitted};
    let item = omitted(ItemKind::Constant, "MT_RAND_PHP").ok_or_else(|| {
        phera_extension_api::NativeError::error("Missing MT_RAND_PHP deprecation")
    })?;
    Err(phera_extension_api::NativeError::throw(
        "Phera\\NotSupportedException",
        item.unsupported_message(),
    ))
}

/// The state is 624 words of 32 bits.
const N: usize = 624;
/// The twist combines each word with the one this far ahead.
const M: usize = 397;

#[derive(Clone)]
pub struct Mt19937 {
    state: [u32; N],
    /// The next word to temper; `N` when the state needs a twist.
    index: usize,
}

impl Mt19937 {
    pub fn new(seed: u32) -> Self {
        let mut state = [0; N];
        state[0] = seed;
        for i in 1..N {
            let previous = state[i - 1];
            state[i] = 1812433253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: N }
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
            let low_bit = next & 1;
            // Mix in the matrix when the low bit is set, without a branch.
            self.state[i] = self.state[(i + M) % N]
                ^ (((word & 0x80000000) | (next & 0x7fffffff)) >> 1)
                ^ (0x9908b0df & 0u32.wrapping_sub(low_bit));
        }
        self.index = 0;
    }
}
