//! `socket_create_pair(AF_UNIX, SOCK_STREAM, 0, $pair)` returns two connected sockets.

use std::{
    net::{Ipv4Addr, Shutdown, SocketAddrV4},
    sync::Arc,
};

use nix::libc;
use phera_ext_streams::StreamSocket;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};
use socket2::{Domain, Protocol, Socket, Type};

use crate::{PORT, SOCKET, classes, failure};

const DOMAIN: Param = Param::new("domain", ParamType::Int);
const TYPE: Param = Param::new("type", ParamType::Int);
const PROTOCOL: Param = Param::new("protocol", ParamType::Int);

pub(crate) fn register(registry: &mut Registry) {
    const BACKLOG: Param = Param::new("backlog", ParamType::Int);
    const ONE: &[Param] = &[SOCKET];
    for entry in [
        FunctionEntry::sync("socket_create", &[DOMAIN, TYPE, PROTOCOL], create),
        FunctionEntry::sync(
            "socket_create_pair",
            const {
                &[
                    DOMAIN,
                    TYPE,
                    PROTOCOL,
                    Param::new("pair", ParamType::Mixed).by_ref(),
                ]
            },
            create_pair,
        ),
        FunctionEntry::sync(
            "socket_create_listen",
            const {
                &[
                    PORT,
                    BACKLOG.with_constant_default(
                        "SOMAXCONN",
                        ConstantValue::Int(libc::SOMAXCONN as i64),
                    ),
                ]
            },
            create_listener,
        ),
        FunctionEntry::asynchronous("socket_accept", ONE, accept),
        FunctionEntry::asynchronous(
            "socket_listen",
            const { &[SOCKET, BACKLOG.with_default(ConstantValue::Int(0))] },
            listen,
        ),
        FunctionEntry::asynchronous(
            "socket_shutdown",
            const {
                &[
                    SOCKET,
                    Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(2)),
                ]
            },
            shutdown,
        ),
        FunctionEntry::asynchronous("socket_close", ONE, close),
        FunctionEntry::sync("socket_set_block", ONE, set_block),
        FunctionEntry::sync("socket_set_nonblock", ONE, set_nonblock),
        FunctionEntry::sync(
            "socket_import_stream",
            const { &[Param::new("stream", ParamType::Mixed)] },
            import,
        ),
        FunctionEntry::sync("socket_export_stream", ONE, export),
    ] {
        registry.function(entry);
    }
}

fn parts(args: &[Value], function: &str) -> Result<(Domain, Type, Option<Protocol>), NativeError> {
    let domain = args::int(args, 0).unwrap_or_default() as i32;
    let valid = matches!(domain, libc::AF_INET | libc::AF_INET6 | libc::AF_UNIX);
    #[cfg(target_os = "linux")]
    let valid = valid || domain == libc::AF_PACKET;
    if !valid {
        let domains = if cfg!(target_os = "linux") {
            "AF_UNIX, AF_PACKET, AF_INET6, or AF_INET"
        } else {
            "AF_UNIX, AF_INET6, or AF_INET"
        };
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($domain) must be one of {domains}"
        )));
    }
    let socket_type = args::int(args, 1).unwrap_or_default() as i32;
    if !matches!(
        socket_type,
        libc::SOCK_STREAM
            | libc::SOCK_DGRAM
            | libc::SOCK_SEQPACKET
            | libc::SOCK_RAW
            | libc::SOCK_RDM
    ) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($type) must be one of SOCK_STREAM, SOCK_DGRAM, SOCK_SEQPACKET, SOCK_RAW, or SOCK_RDM"
        )));
    }
    let protocol = args::int(args, 2).unwrap_or_default() as i32;
    Ok((
        domain.into(),
        socket_type.into(),
        (protocol != 0).then(|| protocol.into()),
    ))
}

fn create(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "socket_create";
    let (domain, kind, protocol) = parts(args, function)?;
    let result = Socket::new(domain, kind, protocol)
        .and_then(|socket| StreamSocket::register_descriptor(socket, domain, kind, false));
    match result {
        Ok(socket) => classes::socket(context, Arc::new(socket), Value::Null),
        Err(error) => failure(context, None, function, "Unable to create socket", &error),
    }
}

fn create_pair(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "socket_create_pair";
    let (domain, kind, protocol) = parts(args, function)?;
    let result = Socket::pair(domain, kind, protocol).and_then(|(left, right)| {
        Ok([
            StreamSocket::register_descriptor(left, domain, kind, false)?,
            StreamSocket::register_descriptor(right, domain, kind, false)?,
        ])
    });
    match result {
        Ok([left, right]) => {
            let left = classes::socket(context, Arc::new(left), Value::Null)?;
            let right = classes::socket(context, Arc::new(right), Value::Null)?;
            if let Some(pair) = args::reference(args, 3) {
                *pair.borrow_mut() =
                    Value::from(phera_extension_api::PhpArray::from_values([left, right], 2));
            }
            Ok(Value::Bool(true))
        }
        Err(error) => failure(
            context,
            None,
            function,
            "Unable to create socket pair",
            &error,
        ),
    }
}

fn create_listener(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "socket_create_listen";
    let port = u16::try_from(args::int(args, 0).unwrap_or_default()).map_err(|_| {
        NativeError::value_error(format!(
            "{function}(): Argument #1 ($port) must be between 0 and 65535"
        ))
    })?;
    let backlog = args::int(args, 1).unwrap_or(i64::from(libc::SOMAXCONN)) as i32;
    let result = (|| {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
        socket.set_reuse_address(true)?;
        socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
        socket.listen(backlog)?;
        StreamSocket::register_descriptor(socket, Domain::IPV4, Type::STREAM, true)
    })();
    match result {
        Ok(socket) => classes::socket(context, Arc::new(socket), Value::Null),
        Err(error) => failure(
            context,
            None,
            function,
            "unable to bind to given address",
            &error,
        ),
    }
}

fn accept(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "socket_accept";
        let handle = classes::argument(&args, function)?;
        let raw = handle.raw(function)?;
        let result = match raw.connect().await {
            Ok(connection) => connection.accept_descriptor(handle.is_blocking()).await,
            Err(error) => Err(error),
        };
        match result {
            Ok(socket) => classes::socket(context, Arc::new(socket), Value::Null),
            Err(error) => failure(context, Some(&handle), function, "Accept failed", &error),
        }
    })
}

fn listen(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "socket_listen";
        let handle = classes::argument(&args, function)?;
        let backlog = args::int(&args, 1).unwrap_or_default() as i32;
        let result = match handle.raw(function)?.connect().await {
            Ok(connection) => connection.listen_descriptor(backlog),
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => failure(
                context,
                Some(&handle),
                function,
                "unable to listen on socket",
                &error,
            ),
        }
    })
}

fn shutdown(_context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "socket_shutdown";
        let handle = classes::argument(&args, function)?;
        let mode = match args::int(&args, 1).unwrap_or(2) {
            0 => Shutdown::Read,
            1 => Shutdown::Write,
            2 => Shutdown::Both,
            _ => {
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #2 ($mode) must be one of SHUT_RD, SHUT_WR or SHUT_RDWR"
                )));
            }
        };
        let result = match handle.raw(function)?.connect().await {
            Ok(connection) => socket2::SockRef::from(connection.as_ref()).shutdown(mode),
            Err(error) => Err(error),
        };
        Ok(Value::Bool(result.is_ok()))
    })
}

fn close(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        classes::argument(&args, "socket_close")?
            .close(context)
            .await
    })
}

fn set_block(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    classes::argument(args, "socket_set_block")?.set_blocking(true);
    Ok(Value::Bool(true))
}

fn set_nonblock(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    classes::argument(args, "socket_set_nonblock")?.set_blocking(false);
    Ok(Value::Bool(true))
}

fn import(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let stream = args::value(args, 0);
    if let Some(socket) = phera_ext_streams::import_socket_stream(stream) {
        return classes::socket(context, socket, stream.clone());
    }
    match stream {
        Value::Resource(resource) if !resource.is_closed() && resource.kind == "stream" => {
            let kind = phera_ext_streams::stream_kind(resource).unwrap_or("STDIO");
            context.warning(format!(
                "socket_import_stream(): Cannot represent a stream of type {kind} as a Socket Descriptor"
            ));
            Ok(Value::Bool(false))
        }
        Value::Resource(_) => Err(NativeError::type_error(
            "socket_import_stream(): supplied resource is not a valid stream resource",
        )),
        other => Err(NativeError::type_error(format!(
            "socket_import_stream(): Argument #1 ($stream) must be of type resource, {} given",
            other.type_name()
        ))),
    }
}

fn export(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    classes::argument(args, "socket_export_stream")?.export(context)
}
