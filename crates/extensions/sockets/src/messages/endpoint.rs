//! UDP messages use a name such as `['addr' => '127.0.0.1', 'port' => 9000]`.

use std::{net::SocketAddr, os::unix::ffi::OsStrExt};

use nix::libc;
use phera_ext_streams::SocketConnection;
use phera_extension_api::{
    ArrayKey, Context, NativeError, PhpArray, Value,
    value::{to_int, to_php_string},
};
use rustix::net::{AddressFamily, SocketAddrAny, SocketAddrUnix};
use socket2::{Domain, SockRef};

pub(super) enum Destination {
    Ip(SocketAddr),
    Unix(SocketAddrUnix),
}

pub(super) async fn parse(
    context: &mut dyn Context,
    socket: &SocketConnection,
    message: &PhpArray,
) -> Result<Result<Option<Destination>, String>, NativeError> {
    let Some(name) = message.get(&ArrayKey::str("name")) else {
        return Ok(Ok(None));
    };
    let name = name.deref();
    let Value::Array(name) = &*name else {
        return Ok(Err("expected an array here".into()));
    };
    let fallback = SockRef::from(socket)
        .local_addr()
        .map_or(libc::AF_UNSPEC, |address| i32::from(address.domain()));
    let family = name
        .get(&ArrayKey::str("family"))
        .map(|family| to_int(family, context.diagnostics()))
        .transpose()?
        .map_or(fallback, |family| family as i32);
    let result = match family {
        libc::AF_UNIX => {
            let Some(path) = name.get(&ArrayKey::str("path")) else {
                return Ok(Err("The key 'path' is required".into()));
            };
            let path = to_php_string(path, context.diagnostics())?;
            SocketAddrUnix::new(std::ffi::OsStr::from_bytes(path.as_bytes()))
                .map(Destination::Unix)
                .map_err(|error| error.to_string())
        }
        libc::AF_INET | libc::AF_INET6 => {
            let Some(address) = name.get(&ArrayKey::str("addr")) else {
                return Ok(Err("The key 'addr' is required".into()));
            };
            let address = to_php_string(address, context.diagnostics())?;
            let address = address.to_string_lossy();
            let port = name
                .get(&ArrayKey::str("port"))
                .map(|port| to_int(port, context.diagnostics()))
                .transpose()?
                .unwrap_or_default() as u16;
            tokio::net::lookup_host((address.as_ref(), port))
                .await
                .ok()
                .and_then(|mut addresses| {
                    addresses.find(|address| address.is_ipv4() == (family == libc::AF_INET))
                })
                .map(Destination::Ip)
                .ok_or_else(|| "invalid address".into())
        }
        _ => Err("unsupported address family".into()),
    };
    Ok(result.map(Some))
}

pub(super) fn received(address: Option<SocketAddrAny>) -> Value {
    let Some(address) = address else {
        return Value::Null;
    };
    let mut result = PhpArray::new();
    match address.address_family() {
        AddressFamily::INET | AddressFamily::INET6 => {
            if let Ok(endpoint) = SocketAddr::try_from(address) {
                let family = Domain::for_address(endpoint);
                result.insert(
                    ArrayKey::str("family"),
                    Value::Int(i64::from(i32::from(family))),
                );
                result.insert(
                    ArrayKey::str("addr"),
                    Value::from(endpoint.ip().to_string()),
                );
                result.insert(
                    ArrayKey::str("port"),
                    Value::Int(i64::from(endpoint.port())),
                );
                if let SocketAddr::V6(address) = endpoint {
                    result.insert(
                        ArrayKey::str("flowinfo"),
                        Value::Int(i64::from(address.flowinfo())),
                    );
                    result.insert(
                        ArrayKey::str("scope_id"),
                        Value::Int(i64::from(address.scope_id())),
                    );
                }
            }
        }
        AddressFamily::UNIX => {
            if let Ok(endpoint) = SocketAddrUnix::try_from(address) {
                result.insert(
                    ArrayKey::str("family"),
                    Value::Int(i64::from(libc::AF_UNIX)),
                );
                if let Some(path) = endpoint.path() {
                    result.insert(ArrayKey::str("path"), Value::from(path.to_bytes().to_vec()));
                }
            }
        }
        _ => (),
    }
    if result.is_empty() {
        Value::Null
    } else {
        Value::from(result)
    }
}
