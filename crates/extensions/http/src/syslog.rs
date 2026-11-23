//! `openlog()`, `syslog()`, `closelog()` and the `LOG_*` constants.
//!
//! ```php
//! openlog('shop', LOG_PID, LOG_USER);
//! syslog(LOG_WARNING, 'stock is low');   // shop[4711] warning: stock is low
//! ```
//!
//! PHP hands these messages to the machine's system logger. Phera writes
//! them to standard error, one line per message, where `error_log()` also
//! writes. Two reasons: the system logger is reached through the C library,
//! which Phera does not call, and a server in a container is expected to
//! log to its output anyway. The facility (`LOG_USER`, `LOG_LOCAL0`, ...) is
//! accepted and has no effect.

use std::io::Write;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::request::{Sapi, sapi};

/// The `LOG_*` constants with the values every Unix gives them.
const CONSTANTS: &[(&str, i64)] = &[
    // How serious a message is.
    ("LOG_EMERG", 0),
    ("LOG_ALERT", 1),
    ("LOG_CRIT", 2),
    ("LOG_ERR", 3),
    ("LOG_WARNING", 4),
    ("LOG_NOTICE", 5),
    ("LOG_INFO", 6),
    ("LOG_DEBUG", 7),
    // Which part of the system it comes from (the facility).
    ("LOG_KERN", 0),
    ("LOG_USER", 8),
    ("LOG_MAIL", 16),
    ("LOG_DAEMON", 24),
    ("LOG_AUTH", 32),
    ("LOG_SYSLOG", 40),
    ("LOG_LPR", 48),
    ("LOG_NEWS", 56),
    ("LOG_UUCP", 64),
    ("LOG_CRON", 72),
    ("LOG_AUTHPRIV", 80),
    ("LOG_LOCAL0", 128),
    ("LOG_LOCAL1", 136),
    ("LOG_LOCAL2", 144),
    ("LOG_LOCAL3", 152),
    ("LOG_LOCAL4", 160),
    ("LOG_LOCAL5", 168),
    ("LOG_LOCAL6", 176),
    ("LOG_LOCAL7", 184),
    // Options of `openlog()`.
    ("LOG_PID", 1),
    ("LOG_CONS", 2),
    ("LOG_ODELAY", 4),
    ("LOG_NDELAY", 8),
    ("LOG_NOWAIT", 16),
    ("LOG_PERROR", 32),
];

const LOG_PID: i64 = 1;

/// The words for the eight levels, by number.
const LEVELS: [&str; 8] = [
    "emergency",
    "alert",
    "critical",
    "error",
    "warning",
    "notice",
    "info",
    "debug",
];

/// What `openlog()` set for the rest of the request.
#[derive(Clone)]
pub(crate) struct Opened {
    /// The name put in front of every message.
    prefix: Vec<u8>,
    /// Whether the process id follows the name.
    with_pid: bool,
}

impl Default for Opened {
    /// What `syslog()` uses when `openlog()` was never called: PHP's
    /// default name.
    fn default() -> Self {
        Self {
            prefix: b"php".to_vec(),
            with_pid: false,
        }
    }
}

pub(super) fn register(registry: &mut Registry) {
    for (name, value) in CONSTANTS {
        registry.constant(name, *value);
    }
    registry.function(FunctionEntry::sync(
        "openlog",
        const {
            &[
                Param::new("prefix", ParamType::String),
                Param::new("flags", ParamType::Int),
                Param::new("facility", ParamType::Int),
            ]
        },
        openlog,
    ));
    registry.function(FunctionEntry::sync("closelog", &[], closelog));
    registry.function(FunctionEntry::asynchronous(
        "syslog",
        const {
            &[
                Param::new("priority", ParamType::Int),
                Param::new("message", ParamType::String),
            ]
        },
        syslog,
    ));
}

/// `openlog(string $prefix, int $flags, int $facility): true`
fn openlog(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let opened = Opened {
        prefix: args::bytes(arguments, 0).to_vec(),
        with_pid: args::int(arguments, 1).unwrap_or(0) & LOG_PID != 0,
    };
    *context.state().get_or_default::<Opened>() = opened;
    Ok(Value::Bool(true))
}

/// `closelog(): true` forgets what `openlog()` set.
fn closelog(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    *context.state().get_or_default::<Opened>() = Opened::default();
    Ok(Value::Bool(true))
}

/// `syslog(int $priority, string $message): true`
///
/// As in PHP, each line of the message is logged on its own and control
/// characters are written as `\x07`, so one message cannot fake another
/// log line.
fn syslog(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        // The priority holds the facility in its upper bits; the level is
        // the lowest three.
        let level = LEVELS[(args::int(&arguments, 0).unwrap_or(0) & 7) as usize];
        let server = match sapi(context) {
            Sapi::Cli => "",
            Sapi::Web => "phera-server: ",
        };
        let opened = context.state().get_or_default::<Opened>();
        let mut heading = server.as_bytes().to_vec();
        heading.extend_from_slice(&opened.prefix);
        if opened.with_pid {
            heading.extend_from_slice(format!("[{}]", std::process::id()).as_bytes());
        }
        heading.extend_from_slice(format!(" {level}: ").as_bytes());

        let mut lines = Vec::new();
        for line in args::bytes(&arguments, 1).split(|&byte| byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            lines.extend_from_slice(&heading);
            for &byte in line {
                if byte < 0x20 || byte == 0x7f {
                    lines.extend_from_slice(format!("\\x{byte:02x}").as_bytes());
                } else {
                    lines.push(byte);
                }
            }
            lines.push(b'\n');
        }
        // Failing to log is not the script's problem, as with the system logger.
        let _ = tokio::task::spawn_blocking(move || std::io::stderr().write_all(&lines)).await;
        Ok(Value::Bool(true))
    })
}
