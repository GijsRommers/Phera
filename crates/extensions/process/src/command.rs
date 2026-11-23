//! `proc_open(['printf', '%s', 'one argument'], ...)` bypasses the shell.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use phera_ext_standard::environment::RequestEnvironment;
use phera_ext_streams::wait_with_execution_limit;
use phera_extension_api::{Context, NativeError, ProcessCancellation, Value, WorkingDirectory};

use super::child::ManagedChild;

pub(crate) fn shell(bytes: &[u8]) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg0("sh").arg("-c");
    command.arg(OsStr::from_bytes(c_prefix(bytes)));
    command
}

/// PHP's shell helpers use libc's popen. The GNU implementation ends option parsing.
pub(crate) fn popen_command(bytes: &[u8]) -> Command {
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        let mut command = Command::new("/bin/sh");
        command.arg0("sh").args(["-c", "--"]);
        command.arg(OsStr::from_bytes(c_prefix(bytes)));
        command
    } else {
        shell(bytes)
    }
}

pub(crate) async fn string(
    context: &mut dyn Context,
    value: &Value,
) -> Result<Vec<u8>, NativeError> {
    let text = context.stringify(value).await?;
    Ok(text.as_bytes().to_vec())
}

pub(crate) fn c_prefix(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or_default()
}

pub(crate) async fn from_value(
    context: &mut dyn Context,
    value: &Value,
) -> Result<(Command, Vec<u8>), NativeError> {
    let Value::Array(items) = value else {
        let Some(text) = value.as_str_bytes() else {
            return Err(NativeError::type_error("proc_open(): Invalid command type"));
        };
        return Ok((shell(text), text.to_vec()));
    };
    let mut words = Vec::with_capacity(items.len());
    for (_, value) in items.iter() {
        let word = string(context, value).await?;
        if word.contains(&0) {
            return Err(NativeError::value_error(format!(
                "Command array element {} contains a null byte",
                words.len() + 1
            )));
        }
        if words.is_empty() && word.is_empty() {
            return Err(NativeError::value_error(
                "First element must contain a non-empty program name",
            ));
        }
        words.push(word);
    }
    let Some((name, arguments)) = words.split_first() else {
        return Err(NativeError::value_error(
            "proc_open(): Argument #1 ($command) must not be empty",
        ));
    };
    let mut command = Command::new(OsStr::from_bytes(name));
    command.args(arguments.iter().map(|word| OsStr::from_bytes(word)));
    Ok((command, name.clone()))
}

pub(crate) async fn configure(
    context: &mut dyn Context,
    command: &mut Command,
    cwd: Option<&[u8]>,
    env: &Value,
    web: bool,
) -> Result<(), NativeError> {
    let directory = context.state().get_or_default::<WorkingDirectory>();
    if let Some(cwd) = cwd {
        let path = Path::new(OsStr::from_bytes(c_prefix(cwd)));
        if path.as_os_str().is_empty() {
            command.current_dir(path);
        } else if let Ok(resolved) = directory.resolve(path) {
            command.current_dir(resolved);
        }
    } else if let Some(path) = directory.path() {
        command.current_dir(path);
    }
    if web {
        command.process_group(0);
        command.stdin(Stdio::null());
    }
    if let Value::Array(values) = env {
        command.env_clear();
        for (key, value) in values.iter() {
            let value = string(context, value).await?;
            if value.is_empty() {
                continue;
            }
            let (name, value) = match key.as_str_bytes() {
                Some(key) if !key.is_empty() => (c_prefix(key).to_vec(), c_prefix(&value)),
                _ => {
                    let value = c_prefix(&value);
                    let Some(split) = value.iter().position(|&byte| byte == b'=') else {
                        continue;
                    };
                    (value[..split].to_vec(), &value[split + 1..])
                }
            };
            if !name.is_empty() && !name.contains(&b'=') {
                command.env(OsString::from_vec(name), OsStr::from_bytes(value));
            }
        }
    } else if let Some(environment) = context.state().get::<RequestEnvironment>() {
        command.env_clear().envs(
            environment
                .effective_environment()
                .into_iter()
                .map(|(name, value)| (OsStr::from_bytes(name), OsStr::from_bytes(value))),
        );
    }
    Ok(())
}

/// Tokio's spawn performs the exec handshake synchronously. Move that handshake
/// off the request thread, then return the Tokio child to the caller's reactor.
pub(crate) async fn spawn(
    context: &mut dyn Context,
    command: Command,
    web: bool,
) -> Result<io::Result<ManagedChild>, NativeError> {
    let runtime = tokio::runtime::Handle::current();
    let cancelled = context
        .state()
        .get_or_default::<ProcessCancellation>()
        .clone();
    wait_with_execution_limit(context, async move {
        tokio::task::spawn_blocking(move || {
            let _reactor = runtime.enter();
            let child = tokio::process::Command::from(command).spawn()?;
            Ok(ManagedChild::new(child, web, cancelled))
        })
        .await
        .map_err(io::Error::other)?
    })
    .await
}

pub(crate) fn reason(error: &io::Error) -> String {
    error.raw_os_error().map_or_else(
        || error.to_string(),
        |number| {
            let description = io::Error::from_raw_os_error(number).to_string();
            description
                .split_once(" (os error ")
                .map_or(description.clone(), |(message, _)| message.to_owned())
        },
    )
}
