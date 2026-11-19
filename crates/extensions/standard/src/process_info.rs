//! `gethostname()` and `php_uname('s')` describe the host serving the request.

use nix::sys::resource::{UsageWho, getrusage as kernel_usage};
use nix::sys::time::TimeValLike;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};

pub(crate) mod memory;
mod script;
mod shell;

pub(crate) fn register(registry: &mut Registry) {
    const MODE: &[Param] =
        &[Param::new("mode", ParamType::String).with_default(ConstantValue::String("a"))];
    const USAGE: &[Param] =
        &[Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(0))];
    const TITLE: &[Param] = &[Param::new("title", ParamType::String)];
    const COMMAND: &[Param] = &[Param::new("command", ParamType::String)];
    for entry in [
        FunctionEntry::sync("gethostname", &[], hostname),
        FunctionEntry::sync("escapeshellcmd", COMMAND, shell::escape),
        FunctionEntry::sync("php_uname", MODE, uname),
        FunctionEntry::sync("sys_getloadavg", &[], load_average),
        FunctionEntry::sync("getrusage", USAGE, resource_usage),
        FunctionEntry::sync("cli_set_process_title", TITLE, set_process_title),
        FunctionEntry::sync("cli_get_process_title", &[], get_process_title),
    ] {
        registry.function(entry);
    }
    memory::register(registry);
    script::register(registry);
}

fn hostname(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let name = rustix::system::uname();
    Ok(Value::from(name.nodename().to_bytes()))
}

fn uname(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = args::string(arguments, 0).map_or(b"a".as_slice(), |value| value.as_bytes());
    let [mode] = mode else {
        return Err(NativeError::value_error(
            "php_uname(): Argument #1 ($mode) must be a single character",
        ));
    };
    let name = rustix::system::uname();
    let result = match mode {
        b's' => name.sysname().to_bytes(),
        b'n' => name.nodename().to_bytes(),
        b'r' => name.release().to_bytes(),
        b'v' => name.version().to_bytes(),
        b'm' => name.machine().to_bytes(),
        b'a' => {
            let fields = [
                name.sysname(),
                name.nodename(),
                name.release(),
                name.version(),
                name.machine(),
            ];
            let result = fields.map(std::ffi::CStr::to_bytes).join(&b' ');
            return Ok(Value::from(result));
        }
        _ => {
            return Err(NativeError::value_error(
                "php_uname(): Argument #1 ($mode) must be one of \"a\", \"m\", \"n\", \"r\", \"s\", or \"v\"",
            ));
        }
    };
    Ok(Value::from(result))
}

fn load_average(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let load = sysinfo::System::load_average();
    let mut result = PhpArray::new();
    for value in [load.one, load.five, load.fifteen] {
        result.push(Value::Float(value))?;
    }
    Ok(Value::from(result))
}

/// The kernel fills counters it supports and supplies zero for the others.
fn resource_usage(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let who = if args::int(arguments, 0) == Some(1) {
        UsageWho::RUSAGE_CHILDREN
    } else {
        UsageWho::RUSAGE_SELF
    };
    let Ok(usage) = kernel_usage(who) else {
        return Ok(Value::Bool(false));
    };
    let user = usage.user_time();
    let system = usage.system_time();
    let mut result = PhpArray::new();
    for (key, value) in [
        ("ru_oublock", usage.block_writes()),
        ("ru_inblock", usage.block_reads()),
        ("ru_msgsnd", usage.ipc_sends()),
        ("ru_msgrcv", usage.ipc_receives()),
        ("ru_maxrss", usage.max_rss()),
        ("ru_ixrss", usage.shared_integral()),
        ("ru_idrss", usage.unshared_data_integral()),
        ("ru_minflt", usage.minor_page_faults()),
        ("ru_majflt", usage.major_page_faults()),
        ("ru_nsignals", usage.signals()),
        ("ru_nvcsw", usage.voluntary_context_switches()),
        ("ru_nivcsw", usage.involuntary_context_switches()),
        ("ru_nswap", usage.full_swaps()),
        (
            "ru_utime.tv_usec",
            user.num_microseconds().rem_euclid(1_000_000),
        ),
        ("ru_utime.tv_sec", user.tv_sec()),
        (
            "ru_stime.tv_usec",
            system.num_microseconds().rem_euclid(1_000_000),
        ),
        ("ru_stime.tv_sec", system.tv_sec()),
    ] {
        result.insert(ArrayKey::str(key), Value::Int(value));
    }
    Ok(Value::from(result))
}

/// Process title changes are process-wide and would affect other requests.
fn set_process_title(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}

fn get_process_title(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(""))
}
