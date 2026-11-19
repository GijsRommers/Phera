//! `posix_getpid()` returns the Unix process identifier.

mod account;
mod constants;
mod filesystem;
mod terminal;
mod times;

use nix::{errno::Errno, unistd};
use phera_extension_api::{
    Context, Extension, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

pub struct Posix;

impl Extension for Posix {
    fn name(&self) -> &'static str {
        "posix"
    }

    fn register(&self, registry: &mut Registry) {
        for entry in [
            FunctionEntry::sync("posix_getpid", &[], |_, _| int(unistd::getpid().as_raw())),
            FunctionEntry::sync("posix_getppid", &[], |_, _| int(unistd::getppid().as_raw())),
            FunctionEntry::sync("posix_getuid", &[], |_, _| int(unistd::getuid().as_raw())),
            FunctionEntry::sync("posix_geteuid", &[], |_, _| int(unistd::geteuid().as_raw())),
            FunctionEntry::sync("posix_getgid", &[], |_, _| int(unistd::getgid().as_raw())),
            FunctionEntry::sync("posix_getegid", &[], |_, _| int(unistd::getegid().as_raw())),
            FunctionEntry::sync(
                "posix_kill",
                const {
                    &[
                        Param::new("process_id", ParamType::Int),
                        Param::new("signal", ParamType::Int),
                    ]
                },
                kill,
            ),
            FunctionEntry::sync(
                "posix_strerror",
                const { &[Param::new("error_code", ParamType::Int)] },
                strerror,
            ),
            FunctionEntry::sync("posix_get_last_error", &[], last_error),
            FunctionEntry::sync("posix_errno", &[], last_error),
        ] {
            registry.function(entry);
        }
        account::register(registry);
        constants::register(registry);
        filesystem::register(registry);
        terminal::register(registry);
        times::register(registry);
    }
}

#[derive(Default)]
struct LastError(i64);

fn fail(context: &mut dyn Context, error: Errno) -> Value {
    context.state().get_or_default::<LastError>().0 = i64::from(error as i32);
    Value::Bool(false)
}

fn last_error(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    int(context.state().get_or_default::<LastError>().0)
}

fn int(value: impl Into<i64>) -> NativeResult {
    Ok(Value::Int(value.into()))
}

fn kill(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let pid = i32::try_from(args::int(arguments, 0).unwrap_or_default()).map_err(|_| {
        NativeError::value_error(format!(
            "posix_kill(): Argument #1 ($process_id) must be between {} and {}",
            // Zend passes an int bound to a zend_long formatter.
            i32::MIN as u32,
            i32::MAX,
        ))
    })?;
    let signal = args::int(arguments, 1).unwrap_or_default() as i32;
    Ok(match super::child::send_signal(pid, signal) {
        Ok(()) => Value::Bool(true),
        Err(error) => fail(context, error),
    })
}

fn strerror(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default() as i32;
    let description = std::io::Error::from_raw_os_error(number).to_string();
    let message = description
        .split_once(" (os error ")
        .map_or(description.as_str(), |(msg, _)| msg);
    Ok(Value::from(message))
}
