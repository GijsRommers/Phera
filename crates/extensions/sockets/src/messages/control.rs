//! `socket_cmsg_space(SOL_SOCKET, SCM_RIGHTS, 1)` sizes a descriptor message.

use std::{
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
};

use nix::libc;
use phera_ext_streams::StreamSocket;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};
use rustix::{
    io::{FdFlags, fcntl_setfd},
    net::{RecvAncillaryBuffer, RecvAncillaryMessage},
};
use socket2::{SockRef, Socket};

use crate::classes;

pub(super) fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("level", ParamType::Int),
        Param::new("type", ParamType::Int),
        Param::new("num", ParamType::Int).with_default(ConstantValue::Int(0)),
    ];
    registry.function(FunctionEntry::sync("socket_cmsg_space", PARAMETERS, space));
}

fn space(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let level = args::int(arguments, 0).unwrap_or_default() as i32;
    let kind = args::int(arguments, 1).unwrap_or_default() as i32;
    let count = args::int(arguments, 2).unwrap_or_default();
    if count < 0 {
        return Err(NativeError::value_error(
            "socket_cmsg_space(): Argument #3 ($num) must be greater than or equal to 0",
        ));
    }
    let size = match (level, kind) {
        (libc::SOL_SOCKET, libc::SCM_RIGHTS) => {
            if count > i64::from(i32::MAX) / 4 - 8 {
                return Ok(Value::Null);
            }
            rustix::cmsg_aligned_space!(ScmRights(count as usize))
        }
        (libc::IPPROTO_IPV6, libc::IPV6_PKTINFO) => {
            rustix::net::__cmsg_aligned_space(std::mem::size_of::<libc::in6_pktinfo>())
        }
        #[cfg(target_os = "linux")]
        (libc::SOL_SOCKET, libc::SCM_CREDENTIALS) => {
            rustix::cmsg_aligned_space!(ScmCredentials(1))
        }
        _ => {
            return Err(NativeError::value_error(format!(
                "Pair level {level} and/or type {kind} is not supported"
            )));
        }
    };
    Ok(Value::Int(size as i64))
}

pub(super) async fn send_descriptors(message: &PhpArray) -> Result<Vec<OwnedFd>, String> {
    let mut descriptors = Vec::new();
    let Some(control) = message.get(&ArrayKey::str("control")) else {
        return Ok(descriptors);
    };
    let value = control.deref();
    let Value::Array(entries) = &*value else {
        return Err("expected an array here".into());
    };
    for entry in entries.values() {
        let entry = entry.deref();
        let Value::Array(entry) = &*entry else {
            return Err("expected an array here".into());
        };
        let level = field(entry, "level")?;
        let kind = field(entry, "type")?;
        if !matches!((&*level.deref(), &*kind.deref()), (Value::Int(level), Value::Int(kind))
            if *level == i64::from(libc::SOL_SOCKET) && *kind == i64::from(libc::SCM_RIGHTS))
        {
            return Err("unsupported control message".into());
        }
        let data = field(entry, "data")?.deref();
        let Value::Array(values) = &*data else {
            return Err("expected an array here".into());
        };
        for value in values.values() {
            let fd = duplicate(value)
                .await
                .ok_or("expected a Socket or a stream with a descriptor")?;
            descriptors.push(fd);
        }
    }
    Ok(descriptors)
}

fn field<'a>(data: &'a PhpArray, key: &str) -> Result<&'a Value, String> {
    data.get(&ArrayKey::str(key))
        .ok_or_else(|| format!("The key '{key}' is required"))
}

async fn duplicate(value: &Value) -> Option<OwnedFd> {
    let value = value.deref();
    let socket = match &*value {
        Value::Resource(resource) => {
            if let Some(fd) = phera_ext_streams::duplicate_descriptor(resource) {
                return Some(fd);
            }
            phera_ext_streams::import_socket_stream(&value)?
        }
        Value::Object(object) if object.class.name() == "Socket" => {
            let handle = classes::argument(std::slice::from_ref(&*value), "socket_sendmsg").ok()?;
            handle.raw("socket_sendmsg").ok()?
        }
        _ => return None,
    };
    socket
        .connect()
        .await
        .ok()?
        .as_fd()
        .try_clone_to_owned()
        .ok()
}

pub(super) fn received(
    context: &mut dyn Context,
    control: &mut RecvAncillaryBuffer<'_>,
) -> Result<PhpArray, NativeError> {
    let mut controls = PhpArray::new();
    for message in control.drain() {
        let RecvAncillaryMessage::ScmRights(fds) = message else {
            continue;
        };
        let mut data = PhpArray::new();
        for fd in fds {
            fcntl_setfd(&fd, FdFlags::CLOEXEC)
                .map_err(|error| NativeError::error(error.to_string()))?;
            data.push(from_descriptor(context, fd)?)?;
        }
        let mut fields = PhpArray::new();
        fields.insert(
            ArrayKey::str("level"),
            Value::Int(i64::from(libc::SOL_SOCKET)),
        );
        fields.insert(
            ArrayKey::str("type"),
            Value::Int(i64::from(libc::SCM_RIGHTS)),
        );
        fields.insert(ArrayKey::str("data"), Value::from(data));
        controls.push(Value::from(fields))?;
    }
    Ok(controls)
}

fn from_descriptor(context: &mut dyn Context, fd: OwnedFd) -> NativeResult {
    let socket = SockRef::from(&fd);
    let result = if let (Ok(address), Ok(kind)) = (socket.local_addr(), socket.r#type()) {
        let socket =
            StreamSocket::register_descriptor(Socket::from(fd), address.domain(), kind, false)
                .map_err(|error| NativeError::error(error.to_string()))?;
        return classes::socket(context, Arc::new(socket), Value::Null);
    } else {
        phera_ext_streams::received_file_stream(context, fd)
    };
    result.map_err(|error| NativeError::error(error.to_string()))
}
