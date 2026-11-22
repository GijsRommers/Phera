//! `socket_getsockname($server, $address, $port)` returns the bound endpoint.

pub(crate) mod info;

use std::{io, net::IpAddr, os::unix::ffi::OsStrExt, path::Path};

use nix::libc;
use phera_ext_streams::SocketConnection;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};
use rustix::net::{AddressFamily, SocketAddrAny, SocketAddrUnix};
use socket2::{Domain, SockAddr, SockRef};

use crate::{ADDRESS, PORT, SOCKET, classes, data, failure};

pub(crate) fn register(registry: &mut Registry) {
    const NAME: &[Param] = &[
        SOCKET,
        Param::new("address", ParamType::Mixed).by_ref(),
        Param::new("port", ParamType::Mixed).by_ref().optional(),
    ];
    for entry in [
        FunctionEntry::asynchronous(
            "socket_bind",
            const { &[SOCKET, ADDRESS, PORT.with_default(ConstantValue::Int(0))] },
            bind,
        ),
        FunctionEntry::asynchronous(
            "socket_connect",
            const { &[SOCKET, ADDRESS, PORT.nullable().optional()] },
            connect,
        ),
        FunctionEntry::asynchronous("socket_getsockname", NAME, local_name),
        FunctionEntry::asynchronous("socket_getpeername", NAME, peer_name),
    ] {
        registry.function(entry);
    }
    info::register(registry);
}

fn connect(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(endpoint_action(context, args, true))
}

fn bind(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(endpoint_action(context, args, false))
}

async fn endpoint_action(
    context: &mut dyn Context,
    args: Vec<Value>,
    connecting: bool,
) -> NativeResult {
    let (function, detail) = if connecting {
        ("socket_connect", "unable to connect")
    } else {
        ("socket_bind", "Unable to bind address")
    };
    let handle = classes::argument(&args, function)?;
    let socket = match data::connection(&handle, function).await? {
        Ok(connection) => connection,
        Err(error) => return failure(context, Some(&handle), function, detail, &error),
    };
    let destination = match destination(&socket, &args, 1, 2, function, connecting).await? {
        Ok(destination) => destination,
        Err(error) => return address_failure(context, &handle, function, &error),
    };
    let result = if connecting {
        socket
            .connect_descriptor(&destination, handle.is_blocking())
            .await
    } else {
        SockRef::from(socket.as_ref()).bind(&destination)
    };
    match result {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => failure(context, Some(&handle), function, detail, &error),
    }
}

pub(crate) async fn destination(
    socket: &SocketConnection,
    arguments: &[Value],
    address: usize,
    port: usize,
    function: &str,
    require_port: bool,
) -> Result<io::Result<SockAddr>, NativeError> {
    let domain = SockRef::from(socket)
        .local_addr()
        .map(|address| address.domain());
    let bytes = args::bytes(arguments, address);
    let value = args::int(arguments, port);
    match domain {
        Ok(Domain::UNIX) => {
            let limit = std::mem::size_of::<libc::sockaddr_un>()
                - std::mem::offset_of!(libc::sockaddr_un, sun_path);
            if function == "socket_sendto" && bytes.len() >= limit {
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #{} ($address) must be less than {limit} bytes",
                    address + 1
                )));
            }
            Ok(SockAddr::unix(Path::new(std::ffi::OsStr::from_bytes(
                bytes,
            ))))
        }
        Ok(domain @ (Domain::IPV4 | Domain::IPV6)) => {
            let ipv4 = domain == Domain::IPV4;
            if require_port && value.is_none() {
                let family = if ipv4 { "AF_INET" } else { "AF_INET6" };
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #{} ($port) cannot be null when the socket type is {family}",
                    port + 1
                )));
            }
            let text = bytes.split(|&byte| byte == 0).next().unwrap_or_default();
            let host = String::from_utf8_lossy(text);
            let port_number = u16::try_from(value.unwrap_or_default()).map_err(|_| {
                NativeError::value_error(format!(
                    "{function}(): Argument #{} ($port) must be between 0 and 65535",
                    port + 1
                ))
            })?;
            let resolved = tokio::net::lookup_host((host.as_ref(), port_number))
                .await
                .and_then(|mut addresses| {
                    addresses
                        .find(|address| address.is_ipv4() == ipv4)
                        .map(SockAddr::from)
                        .ok_or_else(unknown_host)
                })
                .map_err(|_| unknown_host());
            Ok(resolved)
        }
        Err(error) => Ok(Err(error)),
        Ok(_) => Ok(Err(io::Error::from_raw_os_error(libc::EAFNOSUPPORT))),
    }
}

fn unknown_host() -> io::Error {
    io::Error::from_raw_os_error(-10001)
}

pub(crate) fn address_failure(
    context: &mut dyn Context,
    handle: &classes::Handle,
    function: &str,
    error: &io::Error,
) -> NativeResult {
    failure(context, Some(handle), function, "Host lookup failed", error)
}

fn local_name(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(name(context, args, false))
}

fn peer_name(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(name(context, args, true))
}

async fn name(context: &mut dyn Context, args: Vec<Value>, peer: bool) -> NativeResult {
    let (function, detail) = if peer {
        ("socket_getpeername", "unable to retrieve peer name")
    } else {
        ("socket_getsockname", "unable to retrieve socket name")
    };
    let handle = classes::argument(&args, function)?;
    let endpoint = match data::connection(&handle, function).await? {
        Ok(connection) => {
            let socket = SockRef::from(connection.as_ref());
            if peer {
                socket.peer_addr()
            } else {
                socket.local_addr()
            }
        }
        Err(error) => Err(error),
    };
    match endpoint {
        Ok(endpoint) => {
            if let Some(addr) = endpoint.as_socket() {
                assign_ip(&args, addr.ip(), addr.port(), 1, 2);
            } else if endpoint.domain() == Domain::UNIX {
                let path = endpoint
                    .as_pathname()
                    .map_or(&[][..], |path| path.as_os_str().as_bytes());
                assign(&args, 1, Value::from(path.to_vec()));
            }
            Ok(Value::Bool(true))
        }
        Err(error) => failure(context, Some(&handle), function, detail, &error),
    }
}

pub(crate) fn assign_sender(
    args: &[Value],
    sender: Option<&SocketAddrAny>,
    address: usize,
    port: usize,
) {
    if let Some(sender) = sender {
        match sender.address_family() {
            AddressFamily::INET | AddressFamily::INET6 => {
                if let Ok(endpoint) = std::net::SocketAddr::try_from(sender.clone()) {
                    assign_ip(args, endpoint.ip(), endpoint.port(), address, port);
                }
            }
            AddressFamily::UNIX => {
                if let Ok(endpoint) = SocketAddrUnix::try_from(sender.clone()) {
                    let path = endpoint
                        .path()
                        .map(|path| path.to_bytes().to_vec())
                        .unwrap_or_default();
                    assign(args, address, Value::from(path));
                }
            }
            _ => (),
        }
    }
}

fn assign_ip(args: &[Value], ip: IpAddr, number: u16, address: usize, port: usize) {
    assign(args, address, Value::from(ip.to_string()));
    assign(args, port, Value::Int(i64::from(number)));
}

pub(crate) fn assign(args: &[Value], index: usize, value: Value) {
    if let Some(target) = args::reference(args, index) {
        *target.borrow_mut() = value;
    }
}
