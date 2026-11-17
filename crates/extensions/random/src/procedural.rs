//! The procedural functions. `mt_rand()`, `rand()` and the shuffles share
//! one Mt19937 per request, seeded at random until `mt_srand()` seeds it:
//!
//! ```php
//! mt_srand(42);
//! mt_rand(1, 6);      // the same as in PHP
//! rand(6, 1);         // rand() accepts its bounds in either order
//! random_int(1, 6);   // from the operating system, never seeded
//! uniqid('id');       // "id" and 13 hexadecimal digits of the time
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
    prepare_call,
};

use crate::engines::{self, Engine};
use crate::mt19937::{Mode, Mt19937};

pub const FUNCTIONS: &[FunctionEntry] = &[
    MT_SRAND,
    SRAND,
    MT_RAND,
    RAND,
    MT_GETRANDMAX,
    GETRANDMAX,
    RANDOM_INT,
    RANDOM_BYTES,
    LCG_VALUE,
    UNIQID,
];

/// The largest number `mt_rand()` returns without arguments: 31 bits.
const MT_RAND_MAX: i64 = 0x7fff_ffff;

/// What the procedural functions remember during a request.
#[derive(Default)]
struct RequestRandom {
    /// Mt19937, made on first use.
    engine: Option<Engine>,
    /// The two states of `lcg_value()`'s generator, made on first use.
    lcg: Option<[i64; 2]>,
    /// The microsecond of the last `uniqid()`, which must not repeat.
    last_id: u128,
}

/// The request's Mt19937, seeded at random on first use.
pub fn generator(context: &mut dyn Context) -> Result<&mut Engine, NativeError> {
    let state = context.state().get_or_default::<RequestRandom>();
    if state.engine.is_none() {
        let seed = engines::secure_u64()? as u32;
        state.engine = Some(Engine::Mt(Box::new(Mt19937::new(seed, Mode::Standard))));
    }
    Ok(state.engine.as_mut().expect("made above"))
}

/// `mt_srand(?int $seed = null, int $mode = MT_RAND_MT19937): void`
const MT_SRAND: FunctionEntry = FunctionEntry::sync("mt_srand", SEED, seed);

/// `srand(?int $seed = null, int $mode = MT_RAND_MT19937): void`, the same.
const SRAND: FunctionEntry = FunctionEntry::sync("srand", SEED, seed);

const SEED: &[Param] = &[
    Param::new("seed", ParamType::Int).nullable().optional(),
    Param::new("mode", ParamType::Int).optional(),
];

/// A null seed is random, and only the low 32 bits count. Unlike the
/// engine's constructor, an unknown mode means the standard one.
fn seed(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = args::int(arguments, 1)
        .and_then(Mode::from_flag)
        .unwrap_or(Mode::Standard);
    if mode == Mode::Legacy {
        context.deprecated("The MT_RAND_PHP variant of Mt19937 is deprecated".to_string());
    }
    let seed = match args::int(arguments, 0) {
        Some(seed) => seed as u32,
        None => engines::secure_u64()? as u32,
    };
    let engine = Engine::Mt(Box::new(Mt19937::new(seed, mode)));
    context.state().get_or_default::<RequestRandom>().engine = Some(engine);
    Ok(Value::Null)
}

/// `mt_rand(): int` or `mt_rand(int $min, int $max): int`
const MT_RAND: FunctionEntry = FunctionEntry::sync("mt_rand", ZERO_OR_TWO, mt_rand);

/// `rand(): int` or `rand(int $min, int $max): int`
const RAND: FunctionEntry = FunctionEntry::sync("rand", ZERO_OR_TWO, rand);

/// PHP takes either no arguments or both bounds, and counts them before it
/// converts them. A signature cannot say that, so this one accepts any
/// number and [`bounds`] checks.
const ZERO_OR_TWO: &[Param] = &[
    Param::new("min", ParamType::Mixed).optional(),
    Param::new("max", ParamType::Mixed).variadic(),
];

/// The two bounds as ints, or `None` without arguments.
fn bounds(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
) -> Result<Option<(i64, i64)>, NativeError> {
    const BOUNDS: &[Param] = &[
        Param::new("min", ParamType::Int),
        Param::new("max", ParamType::Int),
    ];
    match arguments {
        [] => Ok(None),
        // Already ints, which is the usual case: nothing to check or convert.
        [Value::Int(min), Value::Int(max)] => Ok(Some((*min, *max))),
        _ => {
            let mut prepared = arguments.to_vec();
            prepare_call(function, BOUNDS, &mut prepared, context)?;
            let min = args::int(&prepared, 0).unwrap_or_default();
            let max = args::int(&prepared, 1).unwrap_or_default();
            Ok(Some((min, max)))
        }
    }
}

/// Without arguments, a draw shifted down to 31 bits.
fn unbounded(context: &mut dyn Context) -> NativeResult {
    let (draw, _) = generator(context)?.generate()?;
    Ok(Value::Int((draw >> 1) as i64))
}

fn mt_rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some((min, max)) = bounds(context, arguments, "mt_rand")? else {
        return unbounded(context);
    };
    if min > max {
        return Err(NativeError::value_error(
            "mt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
        ));
    }
    Ok(Value::Int(generator(context)?.integer(min, max)?))
}

/// Like `mt_rand()`, except that bounds in the wrong order are swapped.
fn rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some((min, max)) = bounds(context, arguments, "rand")? else {
        return unbounded(context);
    };
    let (min, max) = if min > max { (max, min) } else { (min, max) };
    Ok(Value::Int(generator(context)?.integer(min, max)?))
}

/// `mt_getrandmax(): int`
const MT_GETRANDMAX: FunctionEntry = FunctionEntry::sync("mt_getrandmax", &[], getrandmax);

/// `getrandmax(): int`, the same.
const GETRANDMAX: FunctionEntry = FunctionEntry::sync("getrandmax", &[], getrandmax);

fn getrandmax(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(MT_RAND_MAX))
}

/// `random_int(int $min, int $max): int`, from the operating system.
const RANDOM_INT: FunctionEntry = FunctionEntry::sync(
    "random_int",
    &[
        Param::new("min", ParamType::Int),
        Param::new("max", ParamType::Int),
    ],
    random_int,
);

fn random_int(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let min = args::int(arguments, 0).unwrap_or_default();
    let max = args::int(arguments, 1).unwrap_or_default();
    if min > max {
        return Err(NativeError::value_error(
            "random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)",
        ));
    }
    // PHP draws nothing when there is only one possible answer.
    if min == max {
        return Ok(Value::Int(min));
    }
    Ok(Value::Int(Engine::Secure.range(min, max)?))
}

/// `length` zero bytes, or PHP's error when there is no memory for them.
pub fn zeroed(length: usize) -> Result<Vec<u8>, NativeError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| NativeError::error("Insufficient memory"))?;
    bytes.resize(length, 0);
    Ok(bytes)
}

/// `random_bytes(int $length): string`, from the operating system.
const RANDOM_BYTES: FunctionEntry = FunctionEntry::sync(
    "random_bytes",
    &[Param::new("length", ParamType::Int)],
    random_bytes,
);

fn random_bytes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    if length < 1 {
        return Err(NativeError::value_error(
            "random_bytes(): Argument #1 ($length) must be greater than 0",
        ));
    }
    let mut bytes = zeroed(length as usize)?;
    engines::fill(&mut bytes)?;
    Ok(Value::from(bytes))
}

/// `lcg_value(): float`, deprecated since PHP 8.4: a float in (0, 1) from
/// L'Ecuyer's combined generator, seeded at random per request.
const LCG_VALUE: FunctionEntry = FunctionEntry::sync("lcg_value", &[], lcg_value);

fn lcg_value(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    /// For each of the two generators `s = a * s % m`: `m / a`, `a`,
    /// `m % a` and `m`. The quotient and remainder let Schrage's method
    /// multiply without overflow.
    const GENERATORS: [(i64, i64, i64, i64); 2] = [
        (53668, 40014, 12211, 2147483563),
        (52774, 40692, 3791, 2147483399),
    ];
    context.deprecated(
        "Function lcg_value() is deprecated since 8.4, use \\Random\\Randomizer::getFloat() instead"
            .to_string(),
    );
    let state = context.state().get_or_default::<RequestRandom>();
    if state.lcg.is_none() {
        let seed = engines::secure_u64()?;
        state.lcg = Some([i64::from(seed as i32), i64::from((seed >> 32) as i32)]);
    }
    let states = state.lcg.as_mut().expect("made above");
    for (s, (q, a, r, m)) in states.iter_mut().zip(GENERATORS) {
        let k = *s / q;
        *s = a * (*s - q * k) - r * k;
        if *s < 0 {
            *s += m;
        }
    }
    let mut z = states[0] - states[1];
    if z < 1 {
        z += 2147483562;
    }
    // PHP's constant: just under 1 / 2147483563.
    Ok(Value::Float(z as f64 * 4.656613e-10))
}

/// `uniqid(string $prefix = "", bool $more_entropy = false): string`
const UNIQID: FunctionEntry = FunctionEntry::asynchronous(
    "uniqid",
    &[
        Param::new("prefix", ParamType::String).optional(),
        Param::new("more_entropy", ParamType::Bool).optional(),
    ],
    uniqid,
);

/// The prefix, then the seconds and microseconds in hexadecimal; with
/// `$more_entropy`, ten more characters of a random number below 10. Two
/// calls in one request never return the same id: PHP sleeps until the
/// next microsecond, and Phera yields to other requests until then.
fn uniqid(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let now = loop {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| NativeError::error(error.to_string()))?;
            let state = context.state().get_or_default::<RequestRandom>();
            if state.last_id != now.as_micros() {
                state.last_id = now.as_micros();
                break now;
            }
            tokio::task::yield_now().await;
        };
        // The prefix ends at a NUL byte, as a C string does.
        let prefix = args::bytes(&arguments, 0);
        let mut id = prefix
            .split(|&byte| byte == 0)
            .next()
            .unwrap_or_default()
            .to_vec();
        let time = format!("{:08x}{:05x}", now.as_secs() as u32, now.subsec_micros());
        id.extend_from_slice(time.as_bytes());
        if args::bool(&arguments, 1).unwrap_or(false) {
            let fraction = f64::from(engines::secure_u64()? as u32) / f64::from(u32::MAX);
            id.extend_from_slice(format!("{:.8}", fraction * 10.0).as_bytes());
        }
        Ok(Value::from(id))
    })
}
