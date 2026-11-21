//! `stream_socket_accept($server)` waits for the next client without blocking the worker.

use phera_extension_api::{Context, LocalFuture, NativeResult, PhpArray, Value, args};
use socket2::{Domain, Protocol, Socket as Descriptor, Type};

use super::address::{Address, Transport, name};
use super::client::{open_failure, set_error, timeout, transport_error};
use super::{Connection, Io, Socket, functions::socket_arg, tls, transport, wrap};
use crate::{context_options, errors::reason};

pub(super) fn server(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_server";
        let uri = args::bytes(&args, 0);
        let supplied = args::value(&args, 4);
        context_options::validate(supplied, function, 5)?;
        let flags = args::int(&args, 3).unwrap_or(12);
        set_error(&args, 1, 0, "");
        let result = async {
            let (kind, authority) = Transport::parse(uri).map_err(transport_error)?;
            let address = Address::parse(kind, authority).map_err(transport_error)?;
            transport::bind(&address, flags & 4 != 0, flags & 8 != 0).await
        }
        .await;
        Ok(match result {
            Ok(socket) => wrap(context, Socket::live(socket), uri, supplied),
            Err(error) => open_failure(context, &args, function, uri, 1, error),
        })
    })
}

pub(super) fn accept(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_accept";
        let Some(mut io) = socket_arg(&args, function, "socket", context).await? else {
            context.warning(format!("{function}(): Accept failed: Unknown error"));
            return Ok(Value::Bool(false));
        };
        io.timeout = timeout(context, &args, function, 1)?;
        if !io.socket.listener {
            context.warning(format!("{function}(): Accept failed: Unknown error"));
            return Ok(Value::Bool(false));
        }
        let (socket, address) = match io.wait(transport::accept(&io.socket)).await {
            Ok(accepted) => accepted,
            Err(error) => {
                let message = if error.kind() == std::io::ErrorKind::TimedOut {
                    "Connection timed out".into()
                } else {
                    reason(&error)
                };
                context.warning(format!("{function}(): Accept failed: {message}"));
                return Ok(Value::Bool(false));
            }
        };
        let listener =
            crate::resource::stream_argument(args::value(&args, 0), function, "#1 ($socket)")?;
        let attached = crate::request::files(context)
            .stream_contexts
            .get(&listener.id)
            .map_or(Value::Null, |context| Value::Resource(context.clone()));
        if let Transport::Tls(method) = socket.transport {
            let connection = Io {
                socket: socket.clone(),
                blocked: true,
                ..io
            };
            let options = context_options::for_wrapper(&attached, "ssl");
            let result = match tls::Options::in_context(context, &options, false) {
                Ok(options) => {
                    tls::enable(&connection, &options, options.method.unwrap_or(method) & !1).await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                context.warning(format!("{function}(): {error}"));
                context.warning(format!("{function}(): Failed to enable crypto"));
                return Ok(Value::Bool(false));
            }
        }
        if let Some(output) = args::reference(&args, 2) {
            *output.borrow_mut() = name(&address).map_or(Value::Bool(false), Value::from);
        }
        Ok(wrap(context, Socket::live(socket), b"", &attached))
    })
}

pub(super) fn pair(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let domain = args::int(arguments, 0).unwrap_or_default() as i32;
    let socket_type = args::int(arguments, 1).unwrap_or_default() as i32;
    let protocol = args::int(arguments, 2).unwrap_or_default() as i32;
    let sockets = Descriptor::pair(
        Domain::from(domain),
        Type::from(socket_type),
        (protocol != 0).then(|| Protocol::from(protocol)),
    )
    .and_then(|(left, right)| {
        let transport = if Type::from(socket_type) == Type::DGRAM {
            Transport::UnixDatagram
        } else {
            Transport::Unix
        };
        Ok((
            Connection::new(left, transport, false, true)?,
            Connection::new(right, transport, false, true)?,
        ))
    });
    match sockets {
        Ok((left, right)) => {
            let mut streams = PhpArray::new();
            streams.push(wrap(context, Socket::live(left), b"", &Value::Null))?;
            streams.push(wrap(context, Socket::live(right), b"", &Value::Null))?;
            Ok(Value::from(streams))
        }
        Err(error) => {
            let code = error.raw_os_error().unwrap_or_default();
            context.warning(format!(
                "stream_socket_pair(): Failed to create sockets: [{code}]: {}",
                reason(&error)
            ));
            Ok(Value::Bool(false))
        }
    }
}
