//! `socket_set_option($socket, SOL_SOCKET, SO_REUSEADDR, 1)` changes the live descriptor.

mod multicast;

use std::{io, rc::Rc};

use nix::{
    libc,
    sys::{
        socket::{getsockopt, setsockopt, sockopt},
        time::TimeVal,
    },
};
use phera_ext_streams::SocketConnection;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args, value::to_int,
};
use socket2::SockRef;

use crate::{SOCKET, classes, data, failure, status};

pub(crate) use multicast::{MCAST_JOIN_GROUP, MCAST_LEAVE_GROUP};

pub(crate) fn register(registry: &mut Registry) {
    const LEVEL: Param = Param::new("level", ParamType::Int);
    const OPTION: Param = Param::new("option", ParamType::Int);
    const GET: &[Param] = &[SOCKET, LEVEL, OPTION];
    const SET: &[Param] = &[SOCKET, LEVEL, OPTION, Param::new("value", ParamType::Mixed)];
    for entry in [
        FunctionEntry::asynchronous("socket_get_option", GET, get_option),
        FunctionEntry::asynchronous("socket_getopt", GET, getopt),
        FunctionEntry::asynchronous("socket_set_option", SET, set_option),
        FunctionEntry::asynchronous("socket_setopt", SET, setopt),
    ] {
        registry.function(entry);
    }
}

fn get_option(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(get(context, args, "socket_get_option"))
}

fn getopt(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(get(context, args, "socket_getopt"))
}

fn set_option(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(set(context, args, "socket_set_option"))
}

fn setopt(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(set(context, args, "socket_setopt"))
}

async fn get(context: &mut dyn Context, args: Vec<Value>, function: &str) -> NativeResult {
    const DETAIL: &str = "Unable to retrieve socket option";
    let handle = classes::argument(&args, function)?;
    let connection = match data::connection(&handle, function).await? {
        Ok(connection) => connection,
        Err(error) => return failure(context, Some(&handle), function, DETAIL, &error),
    };
    let level = args::int(&args, 1).unwrap_or_default() as i32;
    let option = args::int(&args, 2).unwrap_or_default() as i32;
    match get_value(&connection, level, option) {
        Ok(value) => Ok(value),
        Err(error) => failure(context, Some(&handle), function, DETAIL, &error),
    }
}

fn get_value(connection: &SocketConnection, level: i32, option: i32) -> io::Result<Value> {
    let socket = SockRef::from(connection);
    let number = match (level, option) {
        (libc::SOL_SOCKET, libc::SO_LINGER) => {
            let linger = getsockopt(connection, sockopt::Linger)?;
            return Ok(pair(
                ("l_onoff", i64::from(linger.l_onoff)),
                ("l_linger", i64::from(linger.l_linger)),
            ));
        }
        (libc::SOL_SOCKET, libc::SO_RCVTIMEO | libc::SO_SNDTIMEO) => {
            let duration = if option == libc::SO_RCVTIMEO {
                getsockopt(connection, sockopt::ReceiveTimeout)?
            } else {
                getsockopt(connection, sockopt::SendTimeout)?
            };
            return Ok(pair(("sec", duration.tv_sec()), ("usec", usecs(duration))));
        }
        (libc::SOL_SOCKET, libc::SO_REUSEADDR) => socket_flag(socket.reuse_address()?, option),
        (libc::SOL_SOCKET, libc::SO_REUSEPORT) => socket_flag(socket.reuse_port()?, option),
        (libc::SOL_SOCKET, libc::SO_KEEPALIVE) => socket_flag(socket.keepalive()?, option),
        (libc::SOL_SOCKET, libc::SO_BROADCAST) => socket_flag(socket.broadcast()?, option),
        (libc::SOL_SOCKET, libc::SO_OOBINLINE) => socket_flag(socket.out_of_band_inline()?, option),
        (libc::SOL_SOCKET, libc::SO_DONTROUTE) => {
            socket_flag(getsockopt(connection, sockopt::DontRoute)?, option)
        }
        (libc::SOL_SOCKET, libc::SO_SNDBUF) => socket.send_buffer_size()? as i64,
        (libc::SOL_SOCKET, libc::SO_RCVBUF) => socket.recv_buffer_size()? as i64,
        (libc::SOL_SOCKET, libc::SO_TYPE) => i64::from(i32::from(socket.r#type()?)),
        (libc::SOL_SOCKET, libc::SO_ERROR) => i64::from(
            socket
                .take_error()?
                .and_then(|error| error.raw_os_error())
                .unwrap_or_default(),
        ),
        (libc::IPPROTO_TCP, libc::TCP_NODELAY) => i64::from(socket.tcp_nodelay()?),
        (libc::IPPROTO_IP, libc::IP_TTL) => i64::from(socket.ttl_v4()?),
        (libc::IPPROTO_IP, libc::IP_MULTICAST_IF) => i64::from(multicast::ipv4_index(&socket)?),
        (libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP) => i64::from(socket.multicast_loop_v4()?),
        (libc::IPPROTO_IP, libc::IP_MULTICAST_TTL) => i64::from(socket.multicast_ttl_v4()?),
        (libc::IPPROTO_IPV6, libc::IPV6_V6ONLY) => i64::from(socket.only_v6()?),
        (libc::IPPROTO_IPV6, libc::IPV6_UNICAST_HOPS) => i64::from(socket.unicast_hops_v6()?),
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_IF) => i64::from(socket.multicast_if_v6()?),
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_LOOP) => i64::from(socket.multicast_loop_v6()?),
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_HOPS) => i64::from(socket.multicast_hops_v6()?),
        _ => return Err(unknown(level)),
    };
    Ok(Value::Int(number))
}

async fn set(context: &mut dyn Context, args: Vec<Value>, function: &str) -> NativeResult {
    const DETAIL: &str = "Unable to set socket option";
    let handle = classes::argument(&args, function)?;
    let connection = match data::connection(&handle, function).await? {
        Ok(connection) => connection,
        Err(error) => return failure(context, Some(&handle), function, DETAIL, &error),
    };
    let level = args::int(&args, 1).unwrap_or_default() as i32;
    let option = args::int(&args, 2).unwrap_or_default() as i32;
    match set_value(
        context,
        &connection,
        level,
        option,
        args::value(&args, 3),
        function,
    )? {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => failure(context, Some(&handle), function, DETAIL, &error),
    }
}

fn set_value(
    context: &mut dyn Context,
    connection: &SocketConnection,
    level: i32,
    option: i32,
    value: &Value,
    function: &str,
) -> Result<io::Result<()>, NativeError> {
    let socket = SockRef::from(connection);
    match (level, option) {
        (libc::IPPROTO_IP, libc::IP_MULTICAST_IF) => {
            return Ok(multicast::interface_index(value)
                .and_then(multicast::ipv4_address)
                .and_then(|address| socket.set_multicast_if_v4(&address)));
        }
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_IF) => {
            return Ok(multicast::interface_index(value)
                .and_then(|index| socket.set_multicast_if_v6(index)));
        }
        (libc::IPPROTO_IP | libc::IPPROTO_IPV6, MCAST_JOIN_GROUP | MCAST_LEAVE_GROUP) => {
            return multicast::membership(&socket, option, value, function);
        }
        _ => {}
    }
    if level == libc::SOL_SOCKET {
        let case = match option {
            libc::SO_LINGER => Some(("SO_LINGER", "l_onoff", "l_linger")),
            libc::SO_RCVTIMEO => Some(("SO_RCVTIMEO", "sec", "usec")),
            libc::SO_SNDTIMEO => Some(("SO_SNDTIMEO", "sec", "usec")),
            _ => None,
        };
        if let Some((name, first, second)) = case {
            let (a, b) = option_pair(context, value, function, name, first, second)?;
            return Ok(if option == libc::SO_LINGER {
                let linger_field = |name, number| {
                    i32::try_from(number)
                        .ok()
                        .filter(|value| *value >= 0)
                        .ok_or_else(|| {
                            NativeError::value_error(format!(
                                "{function}(): Argument #4 ($value) \"{name}\" must be between 0 and {}",
                                i32::MAX
                            ))
                        })
                };
                let value = libc::linger {
                    l_onoff: linger_field(first, a)?,
                    l_linger: linger_field(second, b)?,
                };
                setsockopt(connection, sockopt::Linger, &value).map_err(Into::into)
            } else if a < 0 || !(0..1_000_000).contains(&b) {
                Err(io::Error::from_raw_os_error(libc::EDOM))
            } else {
                let value = TimeVal::new(a, to_usecs(b));
                match option {
                    libc::SO_RCVTIMEO => setsockopt(connection, sockopt::ReceiveTimeout, &value),
                    _ => setsockopt(connection, sockopt::SendTimeout, &value),
                }
                .map_err(Into::into)
            });
        }
    }
    let number = to_int(value, context.diagnostics())?;
    let enabled = number != 0;
    Ok(match (level, option) {
        (libc::SOL_SOCKET, libc::SO_REUSEADDR) => socket.set_reuse_address(enabled),
        (libc::SOL_SOCKET, libc::SO_REUSEPORT) => socket.set_reuse_port(enabled),
        (libc::SOL_SOCKET, libc::SO_KEEPALIVE) => socket.set_keepalive(enabled),
        (libc::SOL_SOCKET, libc::SO_BROADCAST) => socket.set_broadcast(enabled),
        (libc::SOL_SOCKET, libc::SO_OOBINLINE) => socket.set_out_of_band_inline(enabled),
        (libc::SOL_SOCKET, libc::SO_DONTROUTE) => {
            setsockopt(connection, sockopt::DontRoute, &enabled).map_err(Into::into)
        }
        (libc::SOL_SOCKET, libc::SO_SNDBUF) => socket.set_send_buffer_size(number as usize),
        (libc::SOL_SOCKET, libc::SO_RCVBUF) => socket.set_recv_buffer_size(number as usize),
        (libc::IPPROTO_TCP, libc::TCP_NODELAY) => socket.set_tcp_nodelay(enabled),
        (libc::IPPROTO_IP, libc::IP_TTL) => socket.set_ttl_v4(number as u32),
        (libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP) => socket.set_multicast_loop_v4(enabled),
        (libc::IPPROTO_IP, libc::IP_MULTICAST_TTL) => socket.set_multicast_ttl_v4(number as u32),
        (libc::IPPROTO_IPV6, libc::IPV6_V6ONLY) => socket.set_only_v6(enabled),
        (libc::IPPROTO_IPV6, libc::IPV6_UNICAST_HOPS) => socket.set_unicast_hops_v6(number as u32),
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_LOOP) => socket.set_multicast_loop_v6(enabled),
        (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_HOPS) => {
            socket.set_multicast_hops_v6(number as u32)
        }
        _ => Err(unknown(level)),
    })
}

fn option_pair(
    context: &mut dyn Context,
    value: &Value,
    function: &str,
    name: &str,
    first: &str,
    second: &str,
) -> Result<(i64, i64), NativeError> {
    let fields = option_fields(value, function, name)?;
    let mut field = |key: &str| {
        let value = fields.get(&ArrayKey::str(key)).ok_or_else(|| {
            NativeError::value_error(format!(
                "{function}(): Argument #4 ($value) must have key \"{key}\""
            ))
        })?;
        to_int(value, context.diagnostics()).map_err(NativeError::from)
    };
    Ok((field(first)?, field(second)?))
}

fn option_fields(value: &Value, function: &str, name: &str) -> Result<Rc<PhpArray>, NativeError> {
    match &*value.deref() {
        Value::Array(fields) => Ok(fields.clone()),
        Value::Object(object) => Ok(Rc::new(object.lazy_properties())),
        _ => Err(NativeError::type_error(format!(
            "{function}(): Argument #4 ($value) must be of type array when argument #3 ($option) is {name}, {} given",
            value.type_name()
        ))),
    }
}

fn pair(first: (&str, i64), second: (&str, i64)) -> Value {
    let mut result = PhpArray::new();
    for (name, value) in [first, second] {
        result.insert(ArrayKey::str(name), Value::Int(value));
    }
    Value::from(result)
}

fn unknown(level: i32) -> io::Error {
    if level < 0 {
        status::invalid_argument()
    } else {
        io::Error::from_raw_os_error(libc::ENOPROTOOPT)
    }
}

fn usecs(duration: TimeVal) -> i64 {
    #[cfg(target_vendor = "apple")]
    return i64::from(duration.tv_usec());
    #[cfg(not(target_vendor = "apple"))]
    return duration.tv_usec();
}

fn to_usecs(usecs: i64) -> libc::suseconds_t {
    #[cfg(target_vendor = "apple")]
    return usecs as libc::suseconds_t;
    #[cfg(not(target_vendor = "apple"))]
    return usecs;
}

// Darwin returns the flag bit for SO_* boolean options; Linux returns 1.
fn socket_flag(enabled: bool, option: i32) -> i64 {
    if enabled {
        if cfg!(target_vendor = "apple") {
            i64::from(option)
        } else {
            1
        }
    } else {
        0
    }
}
