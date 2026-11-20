//! `fsockopen('localhost', 25)` and `stream_socket_client('tcp://localhost:25')` connect.

use std::{io, time::Duration};

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use super::address::{Address, Transport};
use super::{Io, default_timeout, tls, transport, wrap};
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
    Box::pin(open(context, args, "pfsockopen", Layout::HostPort))
}

#[derive(Clone, Copy)]
enum Layout {
    Stream,
    HostPort,
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
    };
    context_options::validate(supplied, function, 6)?;
    let deadline = timeout(context, &arguments, function, timeout_at)?;
    let mut uri = args::bytes(&arguments, 0).to_vec();
    if matches!(layout, Layout::HostPort)
        && let Some(port) = args::int(&arguments, 1)
        && port != -1
    {
        uri.extend(format!(":{port}").bytes());
    }
    let attached = if supplied.is_null() {
        context_options::default_for_wrapper(context)
    } else {
        supplied.clone()
    };
    set_error(&arguments, error_at, 0, "");
    let connected = async {
        let (kind, authority) = Transport::parse(&uri).map_err(transport_error)?;
        let address = Address::parse(kind, authority).map_err(transport_error)?;
        if deadline.is_some_and(|wait| wait.is_zero()) && !kind.datagram() && flags & 2 == 0 {
            return Err(io::Error::from_raw_os_error(nix::libc::ETIMEDOUT));
        }
        let operation = transport::connect(&address, flags & 2 != 0);
        let socket = match deadline {
            Some(duration) => tokio::time::timeout(duration, operation)
                .await
                .map_err(|_| io::Error::from_raw_os_error(nix::libc::ETIMEDOUT))??,
            None => operation.await?,
        };
        let _ = socket.peer.set(address.host.to_owned());
        if let Transport::Tls(method) = kind {
            let ssl = context_options::for_wrapper(&attached, "ssl");
            let io = Io {
                socket: socket.clone(),
                blocked: true,
                timeout: deadline,
                chunk: 8192,
            };
            tls::enable(&io, &ssl, method).await?;
        }
        Ok(socket)
    }
    .await;
    match connected {
        Ok(socket) => Ok(wrap(context, socket, &uri, &attached)),
        Err(error) => Ok(open_failure(
            context, &arguments, function, &uri, error_at, error,
        )),
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
