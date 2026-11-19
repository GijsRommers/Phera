//! `time_nanosleep(0, 1000)` and `time_sleep_until(microtime(true) + 0.01)`
//! let the worker serve another request while waiting for the clock.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    const TIME: &[Param] = &[
        Param::new("seconds", ParamType::Int),
        Param::new("nanoseconds", ParamType::Int),
    ];
    const STAMP: &[Param] = &[Param::new("timestamp", ParamType::Float)];
    registry.function(FunctionEntry::asynchronous(
        "time_nanosleep",
        TIME,
        nanosleep,
    ));
    registry.function(FunctionEntry::asynchronous(
        "time_sleep_until",
        STAMP,
        sleep_until,
    ));
}

fn nanosleep(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let seconds = args::int(&arguments, 0).unwrap_or_default();
        let nanos = args::int(&arguments, 1).unwrap_or_default();
        if seconds < 0 {
            return Err(context.native_call_error(
                "time_nanosleep",
                &arguments,
                NativeError::value_error(
                    "time_nanosleep(): Argument #1 ($seconds) must be greater than or equal to 0",
                ),
            ));
        }
        if nanos < 0 {
            return Err(context.native_call_error("time_nanosleep", &arguments, NativeError::value_error(
                "time_nanosleep(): Argument #2 ($nanoseconds) must be greater than or equal to 0",
            )));
        }
        if nanos >= 1_000_000_000 {
            return Err(context.native_call_error(
                "time_nanosleep",
                &arguments,
                NativeError::value_error(
                    "Nanoseconds was not in the range 0 to 999 999 999 or seconds was negative",
                ),
            ));
        }
        let time = Duration::new(seconds as u64, nanos as u32);
        if !time.is_zero() {
            tokio::time::sleep(time).await;
        }
        Ok(Value::Bool(true))
    })
}

fn sleep_until(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stamp = args::float(&arguments, 0).unwrap_or_default();
        if !(0.0..=18_446_744_073.0).contains(&stamp) {
            return Err(NativeError::value_error(
                "time_sleep_until(): Argument #1 ($timestamp) must be between 0 and 18446744073",
            ));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let stop = Duration::from_nanos((stamp * 1e9) as u64);
        let Some(wait) = stop.checked_sub(now) else {
            context.warning(
                "time_sleep_until(): Argument #1 ($timestamp) must be greater than or equal to the current time".into()
            );
            return Ok(Value::Bool(false));
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        Ok(Value::Bool(true))
    })
}
