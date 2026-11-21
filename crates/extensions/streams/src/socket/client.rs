//! `fsockopen('localhost', 25)` and `stream_socket_client('tcp://localhost:25')` connect.

use std::{io, sync::Arc, time::Duration};

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use super::address::{self, Address, Source, Transport};
use super::{
    Socket, default_timeout,
    persistent::{self, Dial, IdKind, Key},
    tls, wrap,
};
use crate::{context_options, errors::reason};

pub(super) fn client(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, args, "stream_socket_client", Layout::Stream))
}

pub(super) fn fsockopen(
    context: &mut dyn Context,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, args, "fsockopen", Layout::HostPort))
}

pub(super) fn pfsockopen(
    context: &mut dyn Context,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(open(context, args, "pfsockopen", Layout::Persistent))
}

#[derive(Clone, Copy)]
enum Layout {
    Stream,
    HostPort,
    Persistent,
}

async fn open(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    layout: Layout,
) -> NativeResult {
    let (error_at, timeout_at, flags, supplied) = match layout {
        Layout::Stream => (
            1,
            3,
            args::int(&arguments, 4).unwrap_or(4),
            args::value(&arguments, 5),
        ),
        Layout::HostPort => (2, 4, 4, &Value::Null),
        Layout::Persistent => (2, 4, 5, &Value::Null),
    };
    context_options::validate(supplied, function, 6)?;
    let deadline = timeout(context, &arguments, function, timeout_at)?;
    let mut uri = args::bytes(&arguments, 0).to_vec();
    let port = args::int(&arguments, 1).unwrap_or(-1);
    if !matches!(layout, Layout::Stream) && port != -1 {
        uri.extend(format!(":{port}").bytes());
    }
    let attached = if supplied.is_null() {
        context_options::default_for_wrapper(context)
    } else {
        supplied.clone()
    };
    set_error(&arguments, error_at, 0, "");
    let connect = async {
        let (kind, authority) = Transport::parse(&uri).map_err(transport_error)?;
        Address::parse(kind, authority).map_err(transport_error)?;
        let source = if kind.unix() {
            None
        } else {
            binding(context, function, &attached)?
        };
        let tls = if matches!(kind, Transport::Tls(_)) {
            Some(tls::Options::in_context(
                context,
                &context_options::for_wrapper(&attached, "ssl"),
                true,
            )?)
        } else {
            None
        };
        let is_persistent = flags & 1 != 0;
        let dial = Dial {
            key: Key {
                application: is_persistent.then(|| context.entry_script_path()).flatten(),
                id: match layout {
                    Layout::Stream => IdKind::Stream,
                    _ => IdKind::HostPort(port),
                },
                uri: uri.clone(),
                source,
                tls,
            },
            deadline,
            kind,
            asynchronous: flags & 2 != 0,
        };
        if !is_persistent {
            let socket = dial
                .open(|message| context.warning(format!("{function}(): {message}")))
                .await?;
            return Ok(wrap(context, Socket::live(socket), &uri, &attached));
        }
        if let Some(value) = persistent::existing(context, &dial.key) {
            return Ok(value);
        }
        let dial = Arc::new(dial);
        let socket = if context.is_snapshot_boot() {
            Socket::deferred(dial.clone())
        } else {
            let lease = dial
                .lease(|message| context.warning(format!("{function}(): {message}")))
                .await?;
            Socket::with_lease(lease)
        };
        let value = wrap(context, socket, &uri, &attached);
        persistent::remember(context, dial, &value);
        Ok(value)
    }
    .await;
    match connect {
        Ok(value) => Ok(value),
        Err(error) => Ok(open_failure(
            context, &arguments, function, &uri, error_at, error,
        )),
    }
}

fn binding(
    context: &mut dyn Context,
    function: &str,
    attached: &Value,
) -> io::Result<Option<Source>> {
    let Some(value) = context_options::option(attached, "socket", "bindto") else {
        return Ok(None);
    };
    let Value::String(value) = &*value.deref() else {
        return Err(transport_error(
            "local_addr context option is not a string.".into(),
        ));
    };
    match address::source(value.as_bytes()) {
        Ok(source) => Ok(source),
        Err(message) => {
            context.warning(format!("{function}(): {message}"));
            Ok(None)
        }
    }
}

pub(super) fn timeout(
    context: &dyn Context,
    arguments: &[Value],
    function: &str,
    index: usize,
) -> Result<Option<Duration>, NativeError> {
    let Some(seconds) = args::float(arguments, index) else {
        return Ok(default_timeout(context));
    };
    if !seconds.is_finite() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{} ($timeout) must be a finite value",
            index + 1
        )));
    }
    Ok(Duration::try_from_secs_f64(seconds).ok())
}

pub(super) fn set_error(arguments: &[Value], index: usize, code: i64, message: &str) {
    if let Some(target) = args::reference(arguments, index) {
        *target.borrow_mut() = Value::Int(code);
    }
    if let Some(target) = args::reference(arguments, index + 1) {
        *target.borrow_mut() = Value::from(message);
    }
}

pub(super) fn transport_error(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub(super) fn open_failure(
    context: &mut dyn Context,
    args: &[Value],
    function: &str,
    uri: &[u8],
    error_at: usize,
    error: io::Error,
) -> Value {
    let code = i64::from(error.raw_os_error().unwrap_or_default());
    let message = reason(&error);
    set_error(args, error_at, code, &message);
    context.warning(format!(
        "{function}(): Unable to connect to {} ({message})",
        String::from_utf8_lossy(uri)
    ));
    Value::Bool(false)
}
