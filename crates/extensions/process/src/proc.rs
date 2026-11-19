//! `proc_get_status($child)['exitcode']` is preserved for subsequent calls and proc_close.

use std::os::unix::process::ExitStatusExt;
use std::rc::Rc;

use phera_ext_streams::{child_stream, new_resource};
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::Resource,
};

use super::child::{ManagedChild, exit_code, send_signal};
use super::command::{bounded, configure, from_value, reason, spawn};

const PROCESS: Param = Param::new("process", ParamType::Mixed);
const SIGTERM: i64 = 15;

pub(crate) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous(
            "proc_open",
            const {
                &[
                    Param::new("command", ParamType::ArrayOrString),
                    Param::new("descriptor_spec", ParamType::Array),
                    Param::new("pipes", ParamType::Mixed).by_ref(),
                    Param::new("cwd", ParamType::String)
                        .nullable()
                        .with_default(ConstantValue::Null),
                    Param::new("env_vars", ParamType::Array)
                        .nullable()
                        .with_default(ConstantValue::Null),
                    Param::new("options", ParamType::Array)
                        .nullable()
                        .with_default(ConstantValue::Null),
                ]
            },
            open,
        ),
        FunctionEntry::asynchronous("proc_close", const { &[PROCESS] }, close),
        FunctionEntry::sync("proc_get_status", const { &[PROCESS] }, status),
        FunctionEntry::sync(
            "proc_terminate",
            const {
                &[
                    PROCESS,
                    Param::new("signal", ParamType::Int).with_default(ConstantValue::Int(SIGTERM)),
                ]
            },
            terminate,
        ),
        FunctionEntry::sync(
            "proc_nice",
            const { &[Param::new("priority", ParamType::Int)] },
            nice,
        ),
    ] {
        registry.function(entry);
    }
}

struct ProcessHandle {
    child: ManagedChild,
    command: Vec<u8>,
    pid: i32,
    pipes: Vec<Rc<Resource>>,
    exit: Option<i64>,
    running: bool,
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        self.close_pipes();
    }
}

impl ProcessHandle {
    fn close_pipes(&mut self) {
        for pipe in self.pipes.drain(..) {
            pipe.data.borrow_mut().take();
        }
    }
}

fn open(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let web = super::permitted(context, "proc_open")?;
        let (mut cmd, command) = from_value(context, args::value(&arguments, 0)).await?;
        let cwd = args::string(&arguments, 3).map(|value| value.as_bytes());
        configure(context, &mut cmd, cwd, args::value(&arguments, 4), web).await?;
        let Value::Array(spec) = args::value(&arguments, 1) else {
            return Err(NativeError::type_error(
                "proc_open(): Invalid descriptor specification",
            ));
        };
        let Some(parents) = super::descriptors::prepare(context, &mut cmd, spec).await? else {
            return Ok(Value::Bool(false));
        };
        let child = match bounded(context, spawn(cmd, true)).await? {
            Ok(child) => child,
            Err(error) => {
                let reason = reason(&error);
                context.warning(format!("proc_open(): posix_spawn() failed: {reason}").into());
                return Ok(Value::Bool(false));
            }
        };
        let pid = child.id().unwrap_or_default() as i32;
        let mut handle = ProcessHandle {
            child: ManagedChild::new(child, web),
            command,
            pid,
            pipes: Vec::new(),
            exit: None,
            running: true,
        };
        let mut pipes = PhpArray::new();
        for pipe in parents {
            match child_stream(context, pipe.fd, pipe.mode, pipe.kind) {
                Ok(Value::Resource(resource)) => {
                    handle.pipes.push(resource.clone());
                    pipes.insert(
                        ArrayKey::Int(i64::from(pipe.number)),
                        Value::Resource(resource),
                    );
                }
                Ok(_) => return Err(NativeError::error("Expected a child stream resource")),
                Err(error) => {
                    context.warning(format!("proc_open(): {}", reason(&error)).into());
                    return Ok(Value::Bool(false));
                }
            }
        }
        if let Some(reference) = args::reference(&arguments, 2) {
            *reference.borrow_mut() = Value::from(pipes);
        }
        Ok(new_resource(context.state(), "process", handle))
    })
}

fn resource<'a>(arguments: &'a [Value], name: &str) -> Result<&'a Resource, NativeError> {
    let value = args::value(arguments, 0);
    let Value::Resource(resource) = value else {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #1 ($process) must be of type resource, {} given",
            super::descriptors::given(value)
        )));
    };
    if resource.kind != "process" || resource.is_closed() {
        return Err(invalid(name));
    }
    Ok(resource)
}

fn invalid(name: &str) -> NativeError {
    NativeError::type_error(format!(
        "{name}(): supplied resource is not a valid process resource"
    ))
}

fn close(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut handle = resource(&arguments, "proc_close")?
            .data
            .borrow_mut()
            .take()
            .and_then(|value| value.downcast::<ProcessHandle>().ok())
            .ok_or_else(|| invalid("proc_close"))?;
        handle.close_pipes();
        let result = if let Some(code) = handle.exit {
            code
        } else if handle.running {
            bounded(context, handle.child.wait())
                .await?
                .map_or(-1, exit_code)
        } else {
            -1
        };
        Ok(Value::Int(result))
    })
}

fn status(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut data = resource(arguments, "proc_get_status")?.data.borrow_mut();
    let handle = data
        .as_mut()
        .and_then(|data| data.downcast_mut::<ProcessHandle>())
        .ok_or_else(|| invalid("proc_get_status"))?;
    let mut signal = 0;
    let mut stopped = 0;
    if handle.running {
        match handle.child.child.try_wait() {
            Ok(None) => {
                use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
                stopped = Pid::from_raw(handle.pid)
                    .and_then(|pid| {
                        waitid(
                            WaitId::Pid(pid),
                            WaitIdOptions::STOPPED | WaitIdOptions::NOHANG,
                        )
                        .ok()
                    })
                    .flatten()
                    .and_then(|status| status.stopping_signal())
                    .unwrap_or_default();
            }
            Ok(Some(status)) => {
                handle.running = false;
                handle.exit = status.code().map(i64::from);
                signal = status.signal().unwrap_or(0);
            }
            Err(_) => handle.running = false,
        }
    }
    let mut array = PhpArray::new();
    for (name, value) in [
        ("command", Value::from(handle.command.clone())),
        ("pid", Value::Int(i64::from(handle.pid))),
        ("cached", Value::Bool(handle.exit.is_some())),
        ("running", Value::Bool(handle.running)),
        ("signaled", Value::Bool(signal != 0)),
        ("stopped", Value::Bool(stopped != 0)),
        ("exitcode", Value::Int(handle.exit.unwrap_or(-1))),
        ("termsig", Value::Int(i64::from(signal))),
        ("stopsig", Value::Int(i64::from(stopped))),
    ] {
        array.insert(ArrayKey::str(name), value);
    }
    Ok(Value::from(array))
}

fn terminate(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let data = resource(arguments, "proc_terminate")?.data.borrow();
    let handle = data
        .as_ref()
        .and_then(|data| data.downcast_ref::<ProcessHandle>())
        .ok_or_else(|| invalid("proc_terminate"))?;
    let signal = args::int(arguments, 1).unwrap_or(SIGTERM) as i32;
    Ok(Value::Bool(
        handle.running && send_signal(handle.pid, signal).is_ok(),
    ))
}

fn nice(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if rustix::process::nice(args::int(arguments, 0).unwrap_or_default() as i32).is_ok() {
        return Ok(Value::Bool(true));
    }
    context.warning(
        "proc_nice(): Only a super user may attempt to increase the priority of a process".into(),
    );
    Ok(Value::Bool(false))
}
