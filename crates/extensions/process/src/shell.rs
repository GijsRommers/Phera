//! `exec('printf hello', $lines, $code)` captures lines and the shell's exit status.

pub(crate) mod popen;

use std::process::Stdio;

use phera_ext_streams::wait_with_execution_limit;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::ChildStdout;

use super::child::exit_code;
use super::command::{configure, popen_command, spawn};

const COMMAND: Param = Param::new("command", ParamType::String).without_nul();
const STATUS: Param = Param::new("result_code", ParamType::Mixed)
    .by_ref()
    .with_default(ConstantValue::Null);

pub(crate) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous("shell_exec", const { &[COMMAND] }, shell_exec),
        FunctionEntry::asynchronous(
            "exec",
            const {
                &[
                    COMMAND,
                    Param::new("output", ParamType::Mixed)
                        .by_ref()
                        .with_default(ConstantValue::Null),
                    STATUS,
                ]
            },
            exec,
        ),
        FunctionEntry::asynchronous("system", const { &[COMMAND, STATUS] }, system),
        FunctionEntry::asynchronous("passthru", const { &[COMMAND, STATUS] }, passthru),
    ] {
        registry.function(entry);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Shell,
    Exec,
    System,
    Passthru,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Shell => "shell_exec",
            Self::Exec => "exec",
            Self::System => "system",
            Self::Passthru => "passthru",
        }
    }
}

fn shell_exec(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, Mode::Shell))
}

fn exec(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, Mode::Exec))
}

fn system(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, Mode::System))
}

fn passthru(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, Mode::Passthru))
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, mode: Mode) -> NativeResult {
    let function = mode.name();
    let web = super::permitted(context, function)?;
    let bytes = args::bytes(&arguments, 0);
    if bytes.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($command) must not be empty"
        )));
    }
    let output = (mode == Mode::Exec)
        .then(|| args::reference(&arguments, 1))
        .flatten();
    let status = args::reference(&arguments, if mode == Mode::Exec { 2 } else { 1 });
    let mut lines = if let Some(reference) = output {
        let value = reference.borrow();
        match &*value {
            Value::Array(array) => array.as_ref().clone(),
            _ => PhpArray::new(),
        }
    } else {
        PhpArray::new()
    };
    let mut command = popen_command(bytes);
    configure(context, &mut command, None, &Value::Null, web).await?;
    command.stdout(Stdio::piped());
    let mut child = match spawn(context, command, web).await? {
        Ok(child) => child,
        Err(_) => {
            let bytes = String::from_utf8_lossy(bytes);
            let detail = if mode == Mode::Shell {
                format!("Unable to execute '{bytes}'")
            } else {
                format!("Unable to fork [{bytes}]")
            };
            context.warning(format!("{function}(): {detail}"));
            if let Some(reference) = status {
                *reference.borrow_mut() = Value::Int(-1);
            }
            if let Some(reference) = output {
                *reference.borrow_mut() = Value::from(lines);
            }
            return Ok(Value::Bool(false));
        }
    };
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| NativeError::error("Child stdout is unavailable"))?;
    let mut stdout = BufReader::new(stdout);
    let value = match mode {
        Mode::Shell | Mode::Passthru => raw(context, &mut stdout, mode).await?,
        Mode::Exec | Mode::System => {
            text(context, &mut stdout, mode, output.map(|_| &mut lines)).await?
        }
    };
    let code = wait_with_execution_limit(context, child.wait())
        .await?
        .map_or(-1, exit_code);
    if let Some(reference) = output {
        *reference.borrow_mut() = Value::from(lines);
    }
    if let Some(reference) = status {
        *reference.borrow_mut() = Value::Int(code);
    }
    Ok(value)
}

async fn raw(
    context: &mut dyn Context,
    stdout: &mut BufReader<ChildStdout>,
    mode: Mode,
) -> NativeResult {
    if mode == Mode::Shell {
        let mut bytes = Vec::new();
        let _ = wait_with_execution_limit(context, stdout.read_to_end(&mut bytes)).await?;
        return Ok(if bytes.is_empty() {
            Value::Null
        } else {
            Value::from(bytes)
        });
    }
    let mut buffer = [0; 8192];
    while let Ok(count @ 1..) = wait_with_execution_limit(context, stdout.read(&mut buffer)).await?
    {
        context.write(&buffer[..count]);
    }
    Ok(Value::Null)
}

async fn text(
    context: &mut dyn Context,
    stdout: &mut BufReader<ChildStdout>,
    mode: Mode,
    mut lines: Option<&mut PhpArray>,
) -> NativeResult {
    let mut line = Vec::new();
    let mut last = Vec::new();
    while let Ok(1..) =
        wait_with_execution_limit(context, stdout.read_until(b'\n', &mut line)).await?
    {
        if mode == Mode::System {
            context.write(&line);
        }
        let length = line
            .iter()
            .rposition(|byte| !b" \t\n\r\x0b\x0c".contains(byte))
            .map_or(0, |position| position + 1);
        line.truncate(length);
        if let Some(output) = &mut lines {
            output.push(Value::from(line.clone()))?;
        }
        std::mem::swap(&mut last, &mut line);
        line.clear();
    }
    Ok(Value::from(last))
}
