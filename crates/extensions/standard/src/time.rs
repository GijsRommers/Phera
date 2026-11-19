//! Clocks and waiting: `time()`, `microtime()`, `hrtime()`, `sleep()`, `usleep()`.
//!
//! ```php
//! $start = hrtime(true);
//! usleep(100_000);                       // wait a tenth of a second
//! echo (hrtime(true) - $start) / 1e9;    // about 0.1
//! ```
//!
//! `sleep` and `usleep` are the simplest functions that *wait* for something
//! outside the program, the way a database query or an HTTP call does. In PHP
//! a waiting script blocks its whole process. Here they are async native
//! functions: while one request waits, the worker thread runs other requests,
//! and a timer wakes this one up again. A thousand requests that each wait
//! 100 ms finish in about 100 ms on a single thread.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use phera_extension_api::{
    Change, Context, FunctionEntry, LanguageLevel, LocalFuture, NativeError, NativeFn,
    NativeResult, Param, ParamType, PhpArray, Value, args,
};

pub(crate) fn functions(level: LanguageLevel) -> [FunctionEntry; 5] {
    let sleep = if level.has(Change::SleepLimit) {
        FunctionEntry {
            handler: NativeFn::Async(sleep::<true>),
            ..SLEEP
        }
    } else {
        SLEEP
    };
    let usleep = if level.has(Change::UsleepLimit) {
        FunctionEntry {
            handler: NativeFn::Async(usleep::<true>),
            ..USLEEP
        }
    } else {
        USLEEP
    };
    [TIME, MICROTIME, HRTIME, sleep, usleep]
}

const MAX_SLEEP_SECONDS: u32 = if cfg!(windows) {
    u32::MAX / 1000
} else {
    u32::MAX
};

/// `time(): int`
const TIME: FunctionEntry = FunctionEntry::sync("time", &[], time);

/// `microtime(bool $as_float = false): string|float`
const MICROTIME: FunctionEntry = FunctionEntry::sync(
    "microtime",
    &[Param::new("as_float", ParamType::Bool).optional()],
    microtime,
);

/// `hrtime(bool $as_number = false): array|int`
const HRTIME: FunctionEntry = FunctionEntry::sync(
    "hrtime",
    &[Param::new("as_number", ParamType::Bool).optional()],
    hrtime,
);

/// `sleep(int $seconds): int`
const SLEEP: FunctionEntry = FunctionEntry::asynchronous(
    "sleep",
    &[Param::new("seconds", ParamType::Int)],
    sleep::<false>,
);

/// `usleep(int $microseconds): void`
const USLEEP: FunctionEntry = FunctionEntry::asynchronous(
    "usleep",
    &[Param::new("microseconds", ParamType::Int)],
    usleep::<false>,
);

/// Time since 1 January 1970 according to the system clock.
fn since_unix_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// `time()`: the current Unix timestamp in whole seconds.
fn time(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(since_unix_epoch().as_secs() as i64))
}

/// `microtime()`: `"0.12345600 1700000000"` (fraction, then seconds), or the
/// same moment as one float when called with `true`.
fn microtime(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let now = since_unix_epoch();
    if args::bool(arguments, 0).unwrap_or(false) {
        return Ok(Value::Float(now.as_secs_f64()));
    }
    let fraction = f64::from(now.subsec_micros()) / 1_000_000.0;
    Ok(Value::from(format!("{fraction:.8} {}", now.as_secs())))
}

/// `hrtime()`: a clock that only moves forward, for measuring how long
/// something took. `[seconds, nanoseconds]`, or nanoseconds as one integer
/// when called with `true`. Like PHP's, it counts from an arbitrary point.
fn hrtime(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    thread_local! {
        /// The moment this worker thread first asked for the time.
        static STARTED: Instant = Instant::now();
    }
    let elapsed = STARTED.with(|started| started.elapsed());
    if args::bool(arguments, 0).unwrap_or(false) {
        return Ok(Value::Int(elapsed.as_nanos() as i64));
    }
    let mut parts = PhpArray::new();
    parts.push(Value::Int(elapsed.as_secs() as i64))?;
    parts.push(Value::Int(i64::from(elapsed.subsec_nanos())))?;
    Ok(Value::from(parts))
}

/// The amount to wait, or PHP's ValueError for a negative argument.
fn wait_amount<const VALIDATE_LIMIT: bool>(
    function: &str,
    parameter: &str,
    amount: i64,
    max: u32,
) -> Result<u64, NativeError> {
    if VALIDATE_LIMIT && !(0..=i64::from(max)).contains(&amount) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 (${parameter}) must be between 0 and {max}"
        )));
    }
    let nonnegative = u64::try_from(amount).map_err(|_| {
        NativeError::value_error(format!(
            "{function}(): Argument #1 (${parameter}) must be greater than or equal to 0"
        ))
    })?;
    // PHP 8.5 passes the value to the platform timer as an unsigned int.
    Ok(u64::from(nonnegative as u32))
}

/// `sleep($seconds)`: waits, letting other requests run meanwhile. Returns 0.
fn sleep<const VALIDATE_LIMIT: bool>(
    _context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let seconds = wait_amount::<VALIDATE_LIMIT>(
            "sleep",
            "seconds",
            args::int(&arguments, 0).unwrap_or(0),
            MAX_SLEEP_SECONDS,
        )?;
        let duration = if cfg!(windows) {
            Duration::from_millis(u64::from((seconds as u32).wrapping_mul(1000)))
        } else {
            Duration::from_secs(seconds)
        };
        tokio::time::sleep(duration).await;
        Ok(Value::Int(0))
    })
}

/// `usleep($microseconds)`: the same in millionths of a second. Returns null.
fn usleep<const VALIDATE_LIMIT: bool>(
    _context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let amount = args::int(&arguments, 0).unwrap_or(0);
        let microseconds =
            wait_amount::<VALIDATE_LIMIT>("usleep", "microseconds", amount, u32::MAX)?;
        tokio::time::sleep(Duration::from_micros(microseconds)).await;
        Ok(Value::Null)
    })
}
