//! CLI scripts can use `SIGTERM`. `pcntl_fork()` throws in the Tokio runtime.

use nix::sys::signal::{SigmaskHow, Signal};
use phera_extension_api::{
    ConstantValue, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry,
};

pub fn register_cli(registry: &mut Registry) {
    register_signals(registry);

    const STATUS: Param = Param::new("status", ParamType::Mixed).by_ref();
    const FLAGS: Param = Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0));
    const USAGE: Param = Param::new("resource_usage", ParamType::Mixed)
        .by_ref()
        .with_default(ConstantValue::EmptyArray);
    for entry in [
        FunctionEntry::sync("pcntl_fork", &[], |_, _| unsupported("pcntl_fork")),
        FunctionEntry::sync(
            "pcntl_exec",
            const {
                &[
                    Param::new("path", ParamType::String).without_nul(),
                    Param::new("args", ParamType::Array).with_default(ConstantValue::EmptyArray),
                    Param::new("env_vars", ParamType::Array)
                        .with_default(ConstantValue::EmptyArray),
                ]
            },
            |_, _| unsupported("pcntl_exec"),
        ),
        FunctionEntry::sync("pcntl_wait", const { &[STATUS, FLAGS, USAGE] }, |_, _| {
            unsupported("pcntl_wait")
        }),
        FunctionEntry::sync(
            "pcntl_waitpid",
            const {
                &[
                    Param::new("process_id", ParamType::Int),
                    STATUS,
                    FLAGS,
                    USAGE,
                ]
            },
            |_, _| unsupported("pcntl_waitpid"),
        ),
        FunctionEntry::sync(
            "pcntl_waitid",
            const {
                &[
                    Param::new("idtype", ParamType::Int).optional(),
                    Param::new("id", ParamType::Int).nullable().optional(),
                    Param::new("info", ParamType::Mixed).by_ref().optional(),
                    FLAGS,
                    USAGE,
                ]
            },
            |_, _| unsupported("pcntl_waitid"),
        ),
    ] {
        registry.function(entry);
    }
}

fn unsupported(function: &str) -> NativeResult {
    let reason = match function {
        "pcntl_fork" => "forking a multithreaded Tokio runtime is unsafe",
        "pcntl_exec" => "replacing the process would replace the entire Tokio runtime",
        _ => "the runtime waits only for child handles it owns",
    };
    Err(NativeError::throw(
        "Phera\\NotSupportedException",
        format!("{function}() is not supported by Phera: {reason}"),
    ))
}

fn register_signals(registry: &mut Registry) {
    const SIGNALS: &[&str] = &[
        "SIGHUP",
        "SIGINT",
        "SIGQUIT",
        "SIGILL",
        "SIGTRAP",
        "SIGABRT",
        "SIGBUS",
        "SIGFPE",
        "SIGKILL",
        "SIGUSR1",
        "SIGSEGV",
        "SIGUSR2",
        "SIGPIPE",
        "SIGALRM",
        "SIGTERM",
        "SIGSTKFLT",
        "SIGCHLD",
        "SIGCONT",
        "SIGSTOP",
        "SIGTSTP",
        "SIGTTIN",
        "SIGTTOU",
        "SIGURG",
        "SIGXCPU",
        "SIGXFSZ",
        "SIGVTALRM",
        "SIGPROF",
        "SIGWINCH",
        "SIGIO",
        "SIGPWR",
        "SIGINFO",
        "SIGSYS",
    ];
    for name in SIGNALS {
        if let Ok(signal) = name.parse::<Signal>() {
            registry.constant(name, i64::from(signal as i32));
        }
    }
    for (name, signal) in [("SIGIOT", Signal::SIGABRT), ("SIGBABY", Signal::SIGSYS)] {
        registry.constant(name, i64::from(signal as i32));
    }
    #[cfg(target_os = "linux")]
    for (name, signal) in [("SIGCLD", Signal::SIGCHLD), ("SIGPOLL", Signal::SIGIO)] {
        registry.constant(name, i64::from(signal as i32));
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::process::Signal as UnixSignal;
        use rustix_libc_wrappers::process::SignalExt;

        registry.constant("SIGRTMIN", i64::from(UnixSignal::rt_min().as_raw()));
        registry.constant("SIGRTMAX", i64::from(UnixSignal::rt_max().as_raw()));
    }
    for (name, value) in [
        ("SIG_DFL", 0),
        ("SIG_IGN", 1),
        ("SIG_ERR", -1),
        ("SIG_BLOCK", SigmaskHow::SIG_BLOCK as i64),
        ("SIG_UNBLOCK", SigmaskHow::SIG_UNBLOCK as i64),
        ("SIG_SETMASK", SigmaskHow::SIG_SETMASK as i64),
    ] {
        registry.constant(name, value);
    }
}
