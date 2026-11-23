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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Value, args,
};

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[TIME, MICROTIME, HRTIME, SLEEP, USLEEP];

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
const SLEEP: FunctionEntry =
    FunctionEntry::asynchronous("sleep", &[Param::new("seconds", ParamType::Int)], sleep);

/// `usleep(int $microseconds): void`
const USLEEP: FunctionEntry = FunctionEntry::asynchronous(
    "usleep",
    &[Param::new("microseconds", ParamType::Int)],
    usleep,
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

/// `hrtime()`: the system's monotonic clock, for measuring how long something
/// took. `[seconds, nanoseconds]`, or nanoseconds as one integer when called
/// with `true`. Like PHP's, it counts from an arbitrary point in the past (the
/// machine's start), so the first call in a script is already a large number.
fn hrtime(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (seconds, nanoseconds) = monotonic_now();
    if args::bool(arguments, 0).unwrap_or(false) {
        return Ok(Value::Int(seconds * 1_000_000_000 + nanoseconds));
    }
    let mut parts = PhpArray::new();
    parts.push(Value::Int(seconds))?;
    parts.push(Value::Int(nanoseconds))?;
    Ok(Value::from(parts))
}

/// The reading of the system's monotonic clock as `(seconds, nanoseconds)`.
/// The clock only moves forward and is not tied to the wall clock, so a
/// changed date does not affect it.
#[cfg(unix)]
fn monotonic_now() -> (i64, i64) {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (now.tv_sec, now.tv_nsec)
}

/// Where the system's clock reading is not available (not Unix), fall back to
/// the time since this process first asked, which also only moves forward.
#[cfg(not(unix))]
fn monotonic_now() -> (i64, i64) {
    use std::sync::OnceLock;
    use std::time::Instant;

    static STARTED: OnceLock<Instant> = OnceLock::new();
    let elapsed = STARTED.get_or_init(Instant::now).elapsed();
    (elapsed.as_secs() as i64, i64::from(elapsed.subsec_nanos()))
}

/// The amount to wait, or PHP's ValueError for a negative argument.
fn wait_amount(function: &str, parameter: &str, amount: i64) -> Result<u64, NativeError> {
    u64::try_from(amount).map_err(|_| {
        NativeError::value_error(format!(
            "{function}(): Argument #1 (${parameter}) must be greater than or equal to 0"
        ))
    })
}

/// `sleep($seconds)`: waits, letting other requests run meanwhile. Returns 0.
fn sleep(_context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let seconds = wait_amount("sleep", "seconds", args::int(&arguments, 0).unwrap_or(0))?;
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok(Value::Int(0))
    })
}

/// `usleep($microseconds)`: the same in millionths of a second. Returns null.
fn usleep(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let amount = args::int(&arguments, 0).unwrap_or(0);
        let microseconds = wait_amount("usleep", "microseconds", amount)
            .map_err(|error| context.native_call_error("usleep", &arguments, error))?;
        tokio::time::sleep(Duration::from_micros(microseconds)).await;
        Ok(Value::Null)
    })
}
