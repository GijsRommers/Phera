//! `stream_socket_get_name($socket, true)` formats the connected address.

use std::net::Shutdown;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use super::{Io, StreamIo, address, client, datagram, server, tls};

const SOCKET: Param = Param::new("socket", ParamType::Mixed);
const STREAM: Param = crate::handles::STREAM;
const ADDRESS: Param = Param::new("address", ParamType::String);
const ERROR_CODE: Param = Param::new("error_code", ParamType::Mixed)
    .by_ref()
    .optional();
const ERROR_MESSAGE: Param = Param::new("error_message", ParamType::Mixed)
    .by_ref()
    .optional();
const TIMEOUT: Param = Param::new("timeout", ParamType::Float)
    .nullable()
    .optional();
const FLAGS: Param = Param::new("flags", ParamType::Int);
const ZERO_FLAGS: Param = FLAGS.with_default(ConstantValue::Int(0));
const CONTEXT: Param = crate::context_options::OPTIONAL_CONTEXT;
const HOST_PORT: &[Param] = &[
    Param::new("hostname", ParamType::String),
    Param::new("port", ParamType::Int).with_default(ConstantValue::Int(-1)),
    ERROR_CODE,
    ERROR_MESSAGE,
    TIMEOUT,
];

pub(crate) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous(
            "stream_socket_client",
            const {
                &[
                    ADDRESS,
                    ERROR_CODE,
                    ERROR_MESSAGE,
                    TIMEOUT,
                    FLAGS.with_default(ConstantValue::Int(4)),
                    CONTEXT,
                ]
            },
            client::client,
        ),
        FunctionEntry::asynchronous("fsockopen", HOST_PORT, client::fsockopen),
        FunctionEntry::asynchronous("pfsockopen", HOST_PORT, client::pfsockopen),
        FunctionEntry::asynchronous(
            "stream_socket_server",
            const {
                &[
                    ADDRESS,
                    ERROR_CODE,
                    ERROR_MESSAGE,
                    FLAGS.with_default(ConstantValue::Int(12)),
                    CONTEXT,
                ]
            },
            server::server,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_accept",
            const {
                &[
                    SOCKET,
                    TIMEOUT,
                    Param::new("peer_name", ParamType::Mixed)
                        .by_ref()
                        .optional(),
                ]
            },
            server::accept,
        ),
        FunctionEntry::sync(
            "stream_socket_pair",
            const {
                &[
                    Param::new("domain", ParamType::Int),
                    Param::new("type", ParamType::Int),
                    Param::new("protocol", ParamType::Int),
                ]
            },
            server::pair,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_get_name",
            const { &[SOCKET, Param::new("remote", ParamType::Bool)] },
            get_name,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_shutdown",
            const { &[STREAM, Param::new("mode", ParamType::Int)] },
            shutdown,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_enable_crypto",
            const {
                &[
                    STREAM,
                    Param::new("enable", ParamType::Bool),
                    Param::new("crypto_method", ParamType::Int)
                        .nullable()
                        .optional(),
                    Param::new("session_stream", ParamType::Mixed).optional(),
                ]
            },
            crypto,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_recvfrom",
            const {
                &[
                    SOCKET,
                    Param::new("length", ParamType::Int),
                    ZERO_FLAGS,
                    Param::new("address", ParamType::Mixed).by_ref().optional(),
                ]
            },
            datagram::receive,
        ),
        FunctionEntry::asynchronous(
            "stream_socket_sendto",
            const {
                &[
                    SOCKET,
                    Param::new("data", ParamType::String),
                    ZERO_FLAGS,
                    ADDRESS.with_default(ConstantValue::String("")),
                ]
            },
            datagram::send,
        ),
        FunctionEntry::sync("stream_get_transports", &[], transports),
    ] {
        registry.function(entry);
    }
    use nix::libc;
    for (name, value) in [
        ("STREAM_CLIENT_PERSISTENT", 1),
        ("STREAM_CLIENT_ASYNC_CONNECT", 2),
        ("STREAM_CLIENT_CONNECT", 4),
        ("STREAM_SERVER_BIND", 4),
        ("STREAM_SERVER_LISTEN", 8),
        ("STREAM_SHUT_RD", 0),
        ("STREAM_SHUT_WR", 1),
        ("STREAM_SHUT_RDWR", 2),
        ("STREAM_PF_INET", libc::AF_INET),
        ("STREAM_PF_INET6", libc::AF_INET6),
        ("STREAM_PF_UNIX", libc::AF_UNIX),
        ("STREAM_SOCK_STREAM", libc::SOCK_STREAM),
        ("STREAM_SOCK_DGRAM", libc::SOCK_DGRAM),
        ("STREAM_SOCK_RAW", libc::SOCK_RAW),
        ("STREAM_SOCK_RDM", libc::SOCK_RDM),
        ("STREAM_SOCK_SEQPACKET", libc::SOCK_SEQPACKET),
        ("STREAM_IPPROTO_IP", libc::IPPROTO_IP),
        ("STREAM_IPPROTO_TCP", libc::IPPROTO_TCP),
        ("STREAM_IPPROTO_UDP", libc::IPPROTO_UDP),
        ("STREAM_IPPROTO_ICMP", libc::IPPROTO_ICMP),
        ("STREAM_IPPROTO_RAW", libc::IPPROTO_RAW),
        ("STREAM_OOB", libc::MSG_OOB),
        ("STREAM_PEEK", libc::MSG_PEEK),
    ] {
        registry.constant(name, i64::from(value));
    }
}

fn transports(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut output = PhpArray::new();
    for &name in address::NAMES {
        output.push(Value::from(name))?;
    }
    Ok(Value::from(output))
}

pub(super) async fn socket_arg(
    args: &[Value],
    function: &str,
    name: &str,
    context: &mut dyn Context,
) -> Result<Option<Io>, NativeError> {
    let resource =
        crate::resource::stream_argument(args::value(args, 0), function, &format!("#1 (${name})"))?;
    let Some(socket) = StreamIo::for_resource(resource) else {
        return Ok(None);
    };
    match socket.resolve().await {
        Ok(io) => Ok(Some(io)),
        Err(error) => {
            context.warning(format!("{function}(): {error}"));
            Ok(None)
        }
    }
}

fn get_name(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(io) = socket_arg(&arguments, "stream_socket_get_name", "socket", context).await?
        else {
            return Ok(Value::Bool(false));
        };
        let descriptor = io.socket.sockref();
        let addr = if args::bool(&arguments, 1).unwrap_or(false) {
            descriptor.peer_addr()
        } else {
            descriptor.local_addr()
        };
        Ok(addr
            .ok()
            .as_ref()
            .and_then(address::name)
            .map_or(Value::Bool(false), Value::from))
    })
}

fn shutdown(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_shutdown";
        let io = socket_arg(&arguments, function, "stream", context).await?;
        let how = match args::int(&arguments, 1) {
            Some(0) => Shutdown::Read,
            Some(1) => Shutdown::Write,
            Some(2) => Shutdown::Both,
            _ => {
                return Err(NativeError::value_error(
                    "stream_socket_shutdown(): Argument #2 ($mode) must be one of \
                STREAM_SHUT_RD, STREAM_SHUT_WR, or STREAM_SHUT_RDWR",
                ));
            }
        };
        Ok(Value::Bool(io.is_some_and(|io| {
            io.socket
                .retired
                .store(true, std::sync::atomic::Ordering::Relaxed);
            io.socket.sockref().shutdown(how).is_ok()
        })))
    })
}

fn crypto(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_enable_crypto";
        let resource = crate::resource::stream_arg(&arguments, function)?;
        let enable = args::bool(&arguments, 1).unwrap_or(false);
        let options = crate::request::files(context)
            .stream_contexts
            .get(&resource.id)
            .map(|value| {
                crate::context_options::for_wrapper(&Value::Resource(value.clone()), "ssl")
            })
            .unwrap_or_default();
        let method = args::int(&arguments, 2).or_else(|| {
            match options.get(&phera_extension_api::ArrayKey::str("crypto_method")) {
                Some(Value::Int(value)) => Some(*value),
                _ => None,
            }
        });
        if enable && method.is_none() {
            return Err(NativeError::value_error(
                "stream_socket_enable_crypto(): Argument #3 ($crypto_method) must be specified when enabling encryption",
            ));
        }
        let io = socket_arg(&arguments, function, "stream", context).await?;
        let Some(io) = io.filter(|io| {
            !io.socket.listener
                && matches!(
                    io.socket.transport,
                    address::Transport::Tcp | address::Transport::Tls(_)
                )
        }) else {
            context.warning(format!(
                "{function}(): This stream does not support SSL/crypto"
            ));
            return Ok(Value::Bool(!enable));
        };
        if !enable {
            tls::disable(&io).await;
            return Ok(Value::Bool(true));
        }
        let method = method.unwrap_or(121);
        let progress = match tls::Options::in_context(context, &options, method & 1 != 0) {
            Ok(options) => {
                if matches!(io.socket.transport, address::Transport::Tcp) {
                    io.socket
                        .retired
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                tls::enable(&io, &options, method).await
            }
            Err(error) => Err(error),
        };
        Ok(match progress {
            Ok(()) => Value::Bool(true),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Value::Int(0),
            Err(error) => {
                context.warning(format!("{function}(): {error}"));
                Value::Bool(false)
            }
        })
    })
}
