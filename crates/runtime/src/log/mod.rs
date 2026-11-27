//! `Phera\Log\Logger`: a structured logger with the PSR-3 methods.
//!
//! ```php
//! use Phera\Log\Logger;
//!
//! $log = new Logger('api', level: 'info', format: 'json', target: '/var/log/api.log');
//! $log->info('User {id} signed in', ['id' => 7]);
//! // {"ts":"2026-11-27T09:15:02.123456+00:00","level":"info","channel":"api","message":"User 7 signed in","context":{"id":7}}
//! $log->debug('skipped');                      // below `info`: returns at once
//! $request = $log->withContext(['request' => $id]);  // same output, more context
//!
//! $test = Logger::memory();                    // keeps records for $test->records()
//! ```
//!
//! A level below the logger's minimum is dropped before the message or the
//! context is looked at. A logger made before `Phera\snapshot()` (or in a
//! router's boot code) works in every forked request. `packages/phera-psr`
//! wraps it in `Psr\Log\LoggerInterface`.

mod record;
mod sink;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, Registry, Value, Visibility,
    WorkingDirectory, args,
};

use record::{Format, Level, Record, Resolution};
use sink::{FileSink, Kept, Sink};

const LOGGER_CLASS: &str = "Phera\\Log\\Logger";
const LEVEL_CLASS: &str = "Phera\\Log\\Level";

/// What a logger and the loggers `withContext()` made from it share.
struct Shared {
    channel: Box<str>,
    min_level: Level,
    format: Format,
    sink: Sink,
}

/// The native state of a `Phera\Log\Logger` object.
#[derive(Clone)]
struct LoggerState {
    shared: Rc<Shared>,
    /// Always an array: merged under every record's own context.
    context: Value,
}

pub(crate) fn register(registry: &mut Registry) {
    const LEVELS: &[(&str, ConstantValue)] = &[
        ("EMERGENCY", ConstantValue::String("emergency")),
        ("ALERT", ConstantValue::String("alert")),
        ("CRITICAL", ConstantValue::String("critical")),
        ("ERROR", ConstantValue::String("error")),
        ("WARNING", ConstantValue::String("warning")),
        ("NOTICE", ConstantValue::String("notice")),
        ("INFO", ConstantValue::String("info")),
        ("DEBUG", ConstantValue::String("debug")),
    ];
    const LEVEL_METHODS: &[MethodEntry] =
        &[MethodEntry::public("__construct", &[], nothing).with_visibility(Visibility::Private)];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        constants: LEVELS,
        methods: LEVEL_METHODS,
        ..ClassEntry::class(LEVEL_CLASS)
    });

    const CHANNEL: Param =
        Param::new("channel", ParamType::String).with_default(ConstantValue::String("app"));
    const FORMAT: Param =
        Param::new("format", ParamType::String).with_default(ConstantValue::String("json"));
    const CONSTRUCT: &[Param] = &[
        CHANNEL,
        Param::new("level", ParamType::String).with_default(ConstantValue::String("info")),
        FORMAT,
        Param::new("target", ParamType::String).with_default(ConstantValue::String("stderr")),
        Param::new("defaultContext", ParamType::Array).with_default(ConstantValue::EmptyArray),
    ];
    const MEMORY: &[Param] = &[
        CHANNEL,
        Param::new("level", ParamType::String).with_default(ConstantValue::String("debug")),
        FORMAT,
    ];
    const CONTEXT: Param =
        Param::new("context", ParamType::Array).with_default(ConstantValue::EmptyArray);
    const MESSAGE: &[Param] = &[Param::new("message", ParamType::String), CONTEXT];
    const LOG: &[Param] = &[
        Param::new("level", ParamType::String),
        Param::new("message", ParamType::String),
        CONTEXT,
    ];
    const WITH_CONTEXT: &[Param] = &[Param::new("context", ParamType::Array)];
    const LEVEL: &[Param] = &[Param::new("level", ParamType::String)];

    /// `$logger->info($message, $context)` and the seven others.
    macro_rules! level_method {
        ($name:literal, $level:expr) => {
            MethodEntry::public_async($name, MESSAGE, |context, this, arguments| {
                Box::pin(log_async(context, this, arguments, Some($level)))
            })
            .with_ready(|context, this, arguments| {
                log_ready(context, this, arguments, Some($level))
            })
            .returning("void")
        };
    }
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT, construct),
        MethodEntry::public("memory", MEMORY, memory)
            .static_()
            .returning("static"),
        level_method!("emergency", Level::Emergency),
        level_method!("alert", Level::Alert),
        level_method!("critical", Level::Critical),
        level_method!("error", Level::Error),
        level_method!("warning", Level::Warning),
        level_method!("notice", Level::Notice),
        level_method!("info", Level::Info),
        level_method!("debug", Level::Debug),
        MethodEntry::public_async("log", LOG, |context, this, arguments| {
            Box::pin(log_async(context, this, arguments, None))
        })
        .with_ready(|context, this, arguments| log_ready(context, this, arguments, None))
        .returning("void"),
        MethodEntry::public("isHandling", LEVEL, is_handling).returning("bool"),
        MethodEntry::public("withContext", WITH_CONTEXT, with_context).returning("static"),
        MethodEntry::public("flush", &[], flush).returning("void"),
        MethodEntry::public("records", &[], records).returning("array"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(LOGGER_CLASS)
    });
}

/// A logger made at boot keeps its configuration and its file in every
/// fork; a memory logger's records are copied, so forks do not see each
/// other's.
const HOOKS: ClassHooks = ClassHooks {
    fork: Some(fork_logger),
    trace_native: Some(trace_logger),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

fn fork_logger(
    _: &Object,
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    let state = state
        .downcast_ref::<LoggerState>()
        .ok_or("its built-in state is not initialized")?;
    let shared = &state.shared;
    let sink = match &shared.sink {
        Sink::Stderr => Sink::Stderr,
        Sink::Stdout => Sink::Stdout,
        Sink::File(file) => Sink::File(file.reopen()),
        Sink::Memory(records) => Sink::Memory(
            records
                .borrow()
                .iter()
                .map(|kept| Kept {
                    time: kept.time.clone(),
                    level: kept.level,
                    message: kept.message.clone(),
                    context: fork.value(&kept.context),
                    line: kept.line.clone(),
                })
                .collect::<Vec<_>>()
                .into(),
        ),
    };
    Ok(Box::new(LoggerState {
        shared: Rc::new(Shared {
            channel: shared.channel.clone(),
            min_level: shared.min_level,
            format: shared.format,
            sink,
        }),
        context: fork.value(&state.context),
    }))
}

fn trace_logger(state: &dyn std::any::Any, visit: &mut dyn FnMut(&Value)) -> bool {
    let Some(state) = state.downcast_ref::<LoggerState>() else {
        return false;
    };
    visit(&state.context);
    if let Sink::Memory(records) = &state.shared.sink {
        for kept in records.borrow().iter() {
            visit(&kept.context);
        }
    }
    true
}

fn nothing(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

fn this_object(this: Option<&Rc<Object>>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Phera\\Log\\Logger method called without an object"))
}

fn not_initialized() -> NativeError {
    NativeError::error("Phera\\Log\\Logger object is not initialized")
}

fn with_state<R>(this: &Object, read: impl FnOnce(&LoggerState) -> R) -> Result<R, NativeError> {
    let native = this.native.borrow();
    let state = native
        .as_ref()
        .and_then(|native| native.downcast_ref::<LoggerState>())
        .ok_or_else(not_initialized)?;
    Ok(read(state))
}

fn level_argument(name: &[u8]) -> Result<Level, NativeError> {
    Level::parse(name).ok_or_else(|| {
        NativeError::throw(
            "InvalidArgumentException",
            format!(
                "\"{}\" is not a PSR-3 log level; use emergency, alert, critical, error, warning, notice, info or debug",
                String::from_utf8_lossy(name)
            ),
        )
    })
}

fn format_argument(name: &[u8]) -> Result<Format, NativeError> {
    Format::parse(name).ok_or_else(|| {
        NativeError::value_error(format!(
            "Phera\\Log\\Logger: format must be \"json\" or \"text\", \"{}\" given",
            String::from_utf8_lossy(name)
        ))
    })
}

/// A relative file path is relative to PHP's working directory, as with
/// `fopen()`.
fn sink_argument(context: &mut dyn Context, target: &[u8]) -> Result<Sink, NativeError> {
    Ok(match target {
        b"stderr" | b"php://stderr" => Sink::Stderr,
        b"stdout" | b"php://stdout" => Sink::Stdout,
        b"memory" => Sink::Memory(Vec::new().into()),
        b"" => {
            return Err(NativeError::value_error(
                "Phera\\Log\\Logger: target must be \"stderr\", \"stdout\", \"memory\" or a file path",
            ));
        }
        path => {
            let path = String::from_utf8_lossy(path).into_owned();
            let file = context
                .state()
                .get_or_default::<WorkingDirectory>()
                .resolve(path.as_ref())
                .and_then(|resolved| FileSink::open(&resolved))
                .map_err(|error| {
                    NativeError::throw(
                        "RuntimeException",
                        format!("Phera\\Log\\Logger: cannot open log file \"{path}\": {error}"),
                    )
                })?;
            Sink::File(file)
        }
    })
}

fn install(object: &Object, state: LoggerState) -> Result<(), NativeError> {
    if object.native.borrow().is_some() {
        return Err(NativeError::throw(
            "LogicException",
            "Phera\\Log\\Logger::__construct() cannot be called twice",
        ));
    }
    *object.native.borrow_mut() = Some(Box::new(state));
    Ok(())
}

fn construct(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let object = this_object(this)?;
    let channel = string_or(arguments, 0, "app");
    let min_level = level_argument(bytes_or(arguments, 1, b"info"))?;
    let format = format_argument(bytes_or(arguments, 2, b"json"))?;
    let sink = sink_argument(context, bytes_or(arguments, 3, b"stderr"))?;
    let context = match args::array(arguments, 4) {
        Some(context) => Value::Array(context.clone()),
        None => Value::from(PhpArray::new()),
    };
    install(
        object,
        LoggerState {
            shared: Rc::new(Shared {
                channel,
                min_level,
                format,
                sink,
            }),
            context,
        },
    )?;
    Ok(Value::Null)
}

/// `Logger::memory()`: a logger that keeps its records for `records()`.
fn memory(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = LoggerState {
        shared: Rc::new(Shared {
            channel: string_or(arguments, 0, "app"),
            min_level: level_argument(bytes_or(arguments, 1, b"debug"))?,
            format: format_argument(bytes_or(arguments, 2, b"json"))?,
            sink: Sink::Memory(Vec::new().into()),
        }),
        context: Value::from(PhpArray::new()),
    };
    new_logger(context, state)
}

fn new_logger(context: &mut dyn Context, state: LoggerState) -> NativeResult {
    let object = context.create_object(LOGGER_CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(state));
    }
    Ok(object)
}

fn bytes_or<'a>(arguments: &'a [Value], index: usize, default: &'a [u8]) -> &'a [u8] {
    if args::given(arguments, index) {
        args::bytes(arguments, index)
    } else {
        default
    }
}

fn string_or(arguments: &[Value], index: usize, default: &str) -> Box<str> {
    String::from_utf8_lossy(bytes_or(arguments, index, default.as_bytes())).into()
}

/// The level, message and context of a call: `info($message, $context)`
/// or `log($level, $message, $context)`.
fn call_parts(arguments: &[Value], level: Option<Level>) -> Result<(Level, usize), NativeError> {
    match level {
        Some(level) => Ok((level, 0)),
        None => Ok((level_argument(args::bytes(arguments, 0))?, 1)),
    }
}

/// The whole call without leaving Rust, when nothing in the context needs
/// PHP to run. A level below the minimum returns before anything else.
fn log_ready(
    context: &mut dyn Context,
    this: &Rc<Object>,
    arguments: &[Value],
    level: Option<Level>,
) -> Option<NativeResult> {
    let (level, message_index) = match call_parts(arguments, level) {
        Ok(parts) => parts,
        Err(error) => return Some(Err(error)),
    };
    let native = this.native.borrow();
    let Some(state) = native
        .as_ref()
        .and_then(|native| native.downcast_ref::<LoggerState>())
    else {
        return Some(Err(not_initialized()));
    };
    if level > state.shared.min_level {
        return Some(Ok(Value::Null));
    }
    let extra = args::array(arguments, message_index + 1);
    if extra.is_some_and(|extra| record::needs_php(&Value::Array(extra.clone()), context, 0)) {
        return None;
    }
    let message = args::bytes(arguments, message_index);
    write(context, state, level, message, extra);
    Some(Ok(Value::Null))
}

/// The same call when the context holds dates, `Stringable` or
/// `JsonSerializable` objects: PHP turns them into plain values first.
async fn log_async(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    level: Option<Level>,
) -> NativeResult {
    let object = this_object(this.as_ref())?;
    let (level, message_index) = call_parts(&arguments, level)?;
    let state = with_state(object, LoggerState::clone)?;
    if level > state.shared.min_level {
        return Ok(Value::Null);
    }
    let extra = match args::array(&arguments, message_index + 1) {
        Some(extra) => match resolve(context, Value::Array(extra.clone()), 0).await? {
            Value::Array(extra) => Some(extra),
            _ => None,
        },
        None => None,
    };
    let message = args::bytes(&arguments, message_index);
    write(context, &state, level, message, extra.as_ref());
    Ok(Value::Null)
}

/// Replaces each object that needs PHP by what PHP makes of it.
fn resolve(
    context: &mut dyn Context,
    value: Value,
    depth: usize,
) -> LocalFuture<'_, Result<Value, NativeError>> {
    Box::pin(async move {
        match value {
            Value::Array(array)
                if record::needs_php(&Value::Array(array.clone()), context, depth) =>
            {
                let mut resolved = PhpArray::with_capacity(array.len());
                for (key, item) in array.iter() {
                    let item = item.deref().clone();
                    resolved.insert(key, resolve(context, item, depth + 1).await?);
                }
                Ok(Value::from(resolved))
            }
            Value::Object(object) => match record::resolution(&object, context) {
                Some(Resolution::Date) => {
                    let format = Value::from("Y-m-d\\TH:i:s.uP");
                    context.call_method(object, "format", vec![format]).await
                }
                Some(Resolution::Json) => {
                    let data = context
                        .call_method(object, "jsonSerialize", Vec::new())
                        .await?;
                    resolve(context, data, depth + 1).await
                }
                Some(Resolution::String) => {
                    let value = Value::Object(object);
                    Ok(Value::String(context.stringify(&value).await?))
                }
                None => Ok(Value::Object(object)),
            },
            Value::Reference(cell) => {
                let inner = cell.borrow().clone();
                resolve(context, inner, depth).await
            }
            other => Ok(other),
        }
    })
}

/// Builds the record and hands it to the sink.
fn write(
    context: &mut dyn Context,
    state: &LoggerState,
    level: Level,
    message: &[u8],
    extra: Option<&Rc<PhpArray>>,
) {
    let merged = merge(&state.context, extra);
    let record = Record::new(level, message, &merged, context);
    let shared = &state.shared;
    let line = record.line(&shared.channel, shared.format, context);
    match &shared.sink {
        Sink::Memory(records) => {
            let mut line = line;
            line.pop();
            records.borrow_mut().push(Kept {
                time: record.time,
                level,
                message: Value::String(record.message),
                context: record.context,
                line: Value::from(line),
            });
        }
        sink => sink.write(line),
    }
}

/// The default context with the call's context over it.
fn merge(default: &Value, extra: Option<&Rc<PhpArray>>) -> Rc<PhpArray> {
    let default = match default {
        Value::Array(default) => default.clone(),
        _ => Rc::new(PhpArray::new()),
    };
    match extra {
        None => default,
        Some(extra) if default.is_empty() => extra.clone(),
        Some(extra) if extra.is_empty() => default,
        Some(extra) => {
            let mut merged = (*default).clone();
            for (key, value) in extra.iter() {
                merged.insert(key, value.deref().clone());
            }
            Rc::new(merged)
        }
    }
}

fn is_handling(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let level = level_argument(args::bytes(arguments, 0))?;
    with_state(this_object(this)?, |state| {
        Value::Bool(level <= state.shared.min_level)
    })
}

/// A logger writing to the same place with `$context` added to its own.
fn with_context(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = with_state(this_object(this)?, LoggerState::clone)?;
    let merged = merge(&state.context, args::array(arguments, 0));
    new_logger(
        context,
        LoggerState {
            shared: state.shared,
            context: Value::Array(merged),
        },
    )
}

/// Waits until every line so far is written.
fn flush(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_state(this_object(this)?, |state| state.shared.sink.flush())?;
    Ok(Value::Null)
}

/// The records of a memory logger, oldest first:
/// `[['ts' => ..., 'level' => 'info', 'channel' => 'app', 'message' => ..., 'context' => [...], 'formatted' => ...]]`.
fn records(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_state(this_object(this)?, |state| {
        let Sink::Memory(records) = &state.shared.sink else {
            return Err(NativeError::throw(
                "LogicException",
                "Phera\\Log\\Logger::records() needs a logger with target \"memory\"",
            ));
        };
        let channel = Value::from(&*state.shared.channel);
        let records = records.borrow();
        let mut list = PhpArray::with_capacity(records.len());
        for kept in records.iter() {
            let mut entry = PhpArray::with_capacity(6);
            entry.insert(ArrayKey::str("ts"), Value::from(kept.time.as_str()));
            entry.insert(ArrayKey::str("level"), Value::from(kept.level.name()));
            entry.insert(ArrayKey::str("channel"), channel.clone());
            entry.insert(ArrayKey::str("message"), kept.message.clone());
            entry.insert(ArrayKey::str("context"), kept.context.clone());
            entry.insert(ArrayKey::str("formatted"), kept.line.clone());
            let _ = list.push(Value::from(entry));
        }
        Ok(Value::from(list))
    })?
}
