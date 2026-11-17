//! `mt_srand(42); mt_rand()` matches PHP's MT19937, including its legacy variant.
//! Ported from php-src/ext/random/engine_mt19937.c; see LICENSE-PHP and LICENSE-MT19937.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Legacy,
}
#[derive(Clone)]
pub struct Mt19937 {
    state: [u32; 624],
    index: usize,
    pub mode: Mode,
}
impl Mt19937 {
    pub fn new(seed: u32, mode: Mode) -> Self {
        let mut state = [0; 624];
        state[0] = seed;
        for i in 1..624 {
            state[i] = 1812433253u32
                .wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self {
            state,
            index: 624,
            mode,
        }
    }
    pub fn next(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let u = self.state[i];
                let v = self.state[(i + 1) % 624];
                let bit = if self.mode == Mode::Legacy {
                    u & 1
                } else {
                    v & 1
                };
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (((u & 0x80000000) | (v & 0x7fffffff)) >> 1)
                    ^ (0x9908b0df & 0u32.wrapping_sub(bit));
            }
            self.index = 0;
        }
        let mut value = self.state[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c5680;
        value ^= (value << 15) & 0xefc60000;
        value ^ (value >> 18)
    }
}
