//! `flock($f, LOCK_EX | LOCK_NB, $wouldBlock)` reports contention without waiting.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};
use rustix::fs::FlockOperation;
use rustix::io::Errno;

use crate::handles::STREAM;
use crate::io_policy::IoPolicy;
use crate::resource::{argument_resource, operate, stream_arg};
use crate::stream::Stream;

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        STREAM,
        Param::new("operation", ParamType::Int),
        Param::new("would_block", ParamType::Mixed)
            .by_ref()
            .optional(),
    ];
    registry.function(FunctionEntry::asynchronous("flock", PARAMS, flock));
    for (name, value) in [
        ("LOCK_SH", 1),
        ("LOCK_EX", 2),
        ("LOCK_UN", 3),
        ("LOCK_NB", 4),
        ("SEEK_SET", 0),
        ("SEEK_CUR", 1),
        ("SEEK_END", 2),
    ] {
        registry.constant(name, value);
    }
}

fn flock(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let candidate = argument_resource(args::value(&arguments, 0), "flock", "#1 ($stream)")?;
        let is_directory = crate::directory::is_open(candidate);
        let resource = if is_directory {
            candidate
        } else {
            stream_arg(&arguments, "flock")?
        };
        let operation = args::int(&arguments, 1).unwrap_or(0);
        let no_wait = operation & 4 != 0;
        let user_operation = operation & 7;
        let operation = match (operation & 3, no_wait) {
            (1, false) => FlockOperation::LockShared,
            (1, true) => FlockOperation::NonBlockingLockShared,
            (2, false) => FlockOperation::LockExclusive,
            (2, true) => FlockOperation::NonBlockingLockExclusive,
            (3, _) => FlockOperation::Unlock,
            _ => {
                return Err(NativeError::value_error(
                    "flock(): Argument #2 ($operation) must be one of LOCK_SH, LOCK_EX, or LOCK_UN",
                ));
            }
        };
        if let Some(file) = crate::user_file::resource_file(resource) {
            let locked = file.lock(context, "flock", user_operation).await?;
            if let Some(output) = args::reference(&arguments, 2) {
                *output.borrow_mut() = Value::Int(0);
            }
            return Ok(Value::Bool(locked));
        }
        let policy = match operation {
            FlockOperation::LockShared | FlockOperation::LockExclusive => IoPolicy::Blocking,
            _ => IoPolicy::Inline,
        };
        let result = if is_directory {
            Err(Errno::NOTSUP)
        } else {
            operate(resource, policy, move |stream: &mut Stream| {
                let file = stream.lock_descriptor().ok_or(Errno::NOTSUP)?;
                rustix::fs::flock(file, operation)?;
                stream.locked = operation != FlockOperation::Unlock;
                Ok(())
            })
            .await?
        };
        if let Some(output) = args::reference(&arguments, 2) {
            *output.borrow_mut() = Value::Int(i64::from(result == Err(Errno::WOULDBLOCK)));
        }
        Ok(Value::Bool(result.is_ok()))
    })
}
