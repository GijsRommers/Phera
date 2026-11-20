//! On Unix, `mail('a@example.test', 'Subject', 'Body')` writes a message to
//! the configured sendmail command and waits for its exit status.

mod headers;
mod log;

use std::process::Stdio;
use std::{io, os::unix::process::ExitStatusExt};

use phera_ext_standard::escape_shell_command;
use phera_ext_streams::wait_with_execution_limit;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry,
    Value, args, ini,
};
use tokio::io::AsyncWriteExt;

use crate::command::{configure, popen_command, reason, spawn};

pub use headers::additional_headers;

pub const RECIPIENT: Param = Param::new("to", ParamType::String).without_nul();
pub const SUBJECT: Param = Param::new("subject", ParamType::String).without_nul();
pub const BODY: Param = Param::new("message", ParamType::String).without_nul();
pub const HEADERS: Param = Param::new("additional_headers", ParamType::ArrayOrString)
    .with_default(ConstantValue::EmptyArray);
pub const PARAMETERS: Param = Param::new("additional_params", ParamType::String).without_nul();

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        RECIPIENT,
        SUBJECT,
        BODY,
        HEADERS,
        PARAMETERS.with_default(ConstantValue::String("")),
    ];
    registry.function(FunctionEntry::asynchronous("mail", PARAMS, mail));
    registry.describe_function("mail", "bool", None);
}

fn mail(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let headers = additional_headers("mail", arguments.get(3))?;
        let message = Message {
            to: normalize_field(args::bytes(&arguments, 0)),
            subject: normalize_field(args::bytes(&arguments, 1)),
            body: args::bytes(&arguments, 2).to_vec(),
            headers,
        };
        deliver(
            context,
            "mail",
            message,
            args::string(&arguments, 4).map(|s| s.as_bytes()),
        )
        .await
    })
}

pub struct Message {
    pub to: Vec<u8>,
    pub subject: Vec<u8>,
    pub body: Vec<u8>,
    pub headers: Vec<u8>,
}

/// Recipient and subject keep RFC folding and replace other control
/// characters with spaces, after trimming trailing C whitespace.
pub fn normalize_field(bytes: &[u8]) -> Vec<u8> {
    let end = bytes
        .iter()
        .rposition(|byte| !b" \t\n\r\x0b\x0c".contains(byte))
        .map_or(0, |i| i + 1);
    let mut field = bytes[..end].to_vec();
    let mut index = 0;
    while index < field.len() {
        if matches!(
            field.get(index..index + 3),
            Some([b'\r', b'\n', b' ' | b'\t'])
        ) {
            index += 3;
            continue;
        }
        if field[index].is_ascii_control() {
            field[index] = b' ';
        }
        index += 1;
    }
    field
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Newline {
    CrLf,
    Lf,
    Mixed,
}

impl Newline {
    fn for_mail(context: &dyn Context) -> Self {
        match context
            .ini_get(b"mail.cr_lf_mode")
            .as_ref()
            .map(|mode| mode.as_bytes())
        {
            Some(b"lf") => Self::Lf,
            Some(b"mixed" | b"os") => Self::Mixed,
            _ if mixed_newlines(context) => Self::Mixed,
            _ => Self::CrLf,
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Self::CrLf => b"\r\n",
            Self::Lf | Self::Mixed => b"\n",
        }
    }
}

pub fn mixed_newlines(context: &dyn Context) -> bool {
    context
        .ini_get(b"mail.mixed_lf_and_crlf")
        .is_some_and(|value| ini::boolean(value.as_bytes()))
}

impl Message {
    fn bytes(&self, newline: Newline) -> Vec<u8> {
        let separator = newline.bytes();
        let mut message = Vec::new();
        message.extend_from_slice(b"To: ");
        message.extend_from_slice(&self.to);
        message.extend_from_slice(separator);
        message.extend_from_slice(b"Subject: ");
        message.extend_from_slice(&self.subject);
        message.extend_from_slice(separator);
        if !self.headers.is_empty() {
            message.extend_from_slice(&self.headers);
            message.extend_from_slice(separator);
        }
        message.extend_from_slice(separator);
        if newline == Newline::Lf {
            let mut bytes = self.body.iter().copied().peekable();
            while let Some(byte) = bytes.next() {
                if byte != b'\r' || bytes.peek() != Some(&b'\n') {
                    message.push(byte);
                }
            }
        } else {
            message.extend_from_slice(&self.body);
        }
        message.extend_from_slice(separator);
        message
    }
}

/// Unix uses sendmail's own sender configuration and does not read sendmail_from.
pub async fn deliver(
    context: &mut dyn Context,
    function: &str,
    mut message: Message,
    extra: Option<&[u8]>,
) -> NativeResult {
    let web = super::permitted(context, function)?;
    let forced = context.ini_value(b"mail.force_extra_parameters");
    let extra = forced.as_ref().map(|value| value.as_bytes()).or(extra);
    let escaped = extra.map(escape_shell_command).transpose()?;
    log::write(context, function, &message).await?;
    let newline = Newline::for_mail(context);
    if context
        .ini_get(b"mail.add_x_header")
        .is_some_and(|value| ini::boolean(value.as_bytes()))
    {
        log::add_origin_header(context, &mut message, newline.bytes()).await;
    }
    if headers::malformed_newlines(&message.headers) {
        context.warning(format!(
            "{function}(): Multiple or malformed newlines found in additional_header"
        ));
        return Ok(Value::Bool(false));
    }
    let Some(path) = context.ini_value(b"sendmail_path") else {
        return Ok(Value::Bool(false));
    };
    let mut command = path.as_bytes().to_vec();
    if let Some(extra) = escaped {
        command.push(b' ');
        command.extend_from_slice(&extra);
    }
    let mut command = popen_command(&command);
    configure(context, &mut command, None, &Value::Null, web).await?;
    command.stdin(Stdio::piped());
    let mut child = match spawn(context, command, web).await? {
        Ok(child) => child,
        Err(error) => {
            failure_to_start(context, function, &path.to_string_lossy(), &error);
            return Ok(Value::Bool(false));
        }
    };
    if let Some(mut input) = child.child.stdin.take() {
        // A sendmail that closes its stdin early still reports delivery through its status.
        let _ =
            wait_with_execution_limit(context, input.write_all(&message.bytes(newline))).await?;
    }
    let failure = match wait_with_execution_limit(context, child.wait()).await? {
        Ok(status) if matches!(status.code(), Some(0 | 75)) => return Ok(Value::Bool(true)),
        Ok(status) => match (status.code(), status.signal()) {
            (Some(code), _) => format!("Sendmail exited with non-zero exit code {code}"),
            (_, Some(signal)) => format!(
                "Sendmail killed by signal {signal} ({})",
                signal_name(signal)
            ),
            _ => "Sendmail did not exit".into(),
        },
        Err(error) => {
            let code = error.raw_os_error().unwrap_or(0);
            format!("Sendmail pclose failed {code} ({})", reason(&error))
        }
    };
    context.warning(format!("{function}(): {failure}"));
    Ok(Value::Bool(false))
}

fn failure_to_start(context: &mut dyn Context, function: &str, path: &str, error: &io::Error) {
    let failure = if error.kind() == io::ErrorKind::PermissionDenied {
        format!("Permission denied: unable to execute shell to run mail delivery binary '{path}'")
    } else {
        format!("Could not execute mail delivery program '{path}'")
    };
    context.warning(format!("{function}(): {failure}"));
}

fn signal_name(signal: i32) -> String {
    use nix::sys::signal::Signal;
    match Signal::try_from(signal) {
        Ok(Signal::SIGHUP) => "Hangup".into(),
        Ok(Signal::SIGINT) => "Interrupt".into(),
        Ok(Signal::SIGQUIT) => "Quit".into(),
        Ok(Signal::SIGKILL) => "Killed".into(),
        Ok(Signal::SIGPIPE) => "Broken pipe".into(),
        Ok(Signal::SIGTERM) => "Terminated".into(),
        Ok(Signal::SIGABRT) => "Aborted".into(),
        Ok(Signal::SIGSEGV) => "Segmentation fault".into(),
        _ => format!("Unknown signal {signal}"),
    }
}
