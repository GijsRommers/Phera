//! Seeded GMP random integers are reproducible for a request.
//! `gmp_random_seed(42); gmp_random_bits(16);`

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::{
    integer::{Argument, Result},
    object,
};

const WORDS: usize = 624;
const WARMUP: usize = 2000;

struct Generator {
    words: [u32; WORDS],
    index: usize,
}

impl Generator {
    fn seeded(seed: BigInt) -> Self {
        let period = BigInt::from(1u8) << 19_937usize;
        let reduced = seed.mod_floor(&(&period - 20_027u32)) + 2u8;
        let mut initial = reduced.modpow(&BigInt::from(1_074_888_996u32), &(period - 20_023u32));
        let mut words = [0; WORDS];
        words[0] = u32::from(initial.bit(19_936)) << 31;
        initial.set_bit(19_936, false);
        for (word, digit) in words[1..].iter_mut().zip(initial.iter_u32_digits()) {
            *word = digit;
        }
        let mut result = Self {
            words,
            index: WARMUP % WORDS,
        };
        for _ in 0..(WARMUP / WORDS) {
            result.twist();
        }
        result
    }

    fn twist(&mut self) {
        for index in 0..WORDS {
            let current = self.words[index] & 0x8000_0000;
            let next = self.words[(index + 1) % WORDS] & 0x7fff_ffff;
            let joined = current | next;
            let feedback = if joined & 1 == 0 { 0 } else { 0x9908_b0df };
            self.words[index] = self.words[(index + 397) % WORDS] ^ (joined >> 1) ^ feedback;
        }
    }

    fn next(&mut self) -> u32 {
        if self.index == WORDS {
            self.twist();
            self.index = 0;
        }
        let mut word = self.words[self.index];
        self.index += 1;
        word ^= word >> 11;
        word ^= (word << 7) & 0x9d2c_5680;
        word ^= (word << 15) & 0xefc6_0000;
        word ^ (word >> 18)
    }

    fn bits(&mut self, bits: u64) -> Result<BigUint> {
        let count = bits.div_ceil(32) as usize;
        let mut digits = Vec::new();
        digits
            .try_reserve_exact(count)
            .map_err(|_| NativeError::error("Could not allocate GMP random integer"))?;
        digits.extend((0..count).map(|_| self.next()));
        let remaining = bits % 32;
        if remaining != 0
            && let Some(last) = digits.last_mut()
        {
            *last &= (1 << remaining) - 1;
        }
        Ok(BigUint::new(digits))
    }
}

#[derive(Default)]
struct Random(Option<Generator>);

fn generator(context: &mut dyn Context) -> Result<&mut Generator> {
    let state = context.state().get_or_default::<Random>();
    if state.0.is_none() {
        let seed = getrandom::u64().map_err(|_| {
            NativeError::throw("Random\\RandomException", "Failed to generate seed")
        })?;
        state.0 = Some(Generator::seeded(seed.into()));
    }
    state
        .0
        .as_mut()
        .ok_or_else(|| NativeError::error("GMP random state is not initialized"))
}

pub(crate) fn seed(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let seed = Argument("gmp_random_seed", 1, "seed").read(values)?;
    context.state().get_or_default::<Random>().0 = Some(Generator::seeded(seed));
    Ok(Value::Null)
}

pub(crate) fn bits(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let bits = args::int(values, 0).unwrap_or(0);
    if !(1..=i64::from(i32::MAX)).contains(&bits) {
        return Err(
            Argument("gmp_random_bits", 1, "bits").value_error("must be between 1 and 2147483647")
        );
    }
    let data = generator(context)?.bits(bits as u64)?;
    object::create(context, data.into())
}

pub(crate) fn range(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let min = Argument("gmp_random_range", 1, "min").read(values)?;
    let max = Argument("gmp_random_range", 2, "max").read(values)?;
    if min >= max {
        return Err(Argument("gmp_random_range", 1, "min")
            .value_error("must be less than argument #2 ($maximum)"));
    }
    let span = &max - &min;
    let limit = span.magnitude();
    let generator = generator(context)?;
    let mut candidate = generator.bits(limit.bits())?;
    for _ in 1..80 {
        if &candidate <= limit {
            break;
        }
        candidate = generator.bits(limit.bits())?;
    }
    if &candidate > limit {
        candidate -= limit + 1u8;
    }
    object::create(context, min + BigInt::from(candidate))
}
