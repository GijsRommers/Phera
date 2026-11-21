//! A message can carry multiple buffers and Unix file descriptors.

mod control;
mod endpoint;

use std::io::{self, IoSlice, IoSliceMut};
use std::mem::MaybeUninit;

use nix::libc;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
    value::{to_int, to_php_string},
};
use rustix::net::{
    RecvAncillaryBuffer, RecvFlags, SendAncillaryBuffer, SendFlags, recvmsg, sendmsg, sendmsg_addr,
};
use tokio::io::Interest;

use crate::{SOCKET, address, classes, data, failure};

pub(crate) fn register(registry: &mut Registry) {
    const MESSAGE: Param = Param::new("message", ParamType::Array);
    const FLAGS: Param = crate::FLAGS.with_default(ConstantValue::Int(0));
    for entry in [
        FunctionEntry::asynchronous("socket_sendmsg", &[SOCKET, MESSAGE, FLAGS], send),
        FunctionEntry::asynchronous(
            "socket_recvmsg",
            const { &[SOCKET, MESSAGE.by_ref(), FLAGS] },
            receive,
        ),
    ] {
        registry.function(entry);
    }
    control::register(registry);
}

fn send(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "socket_sendmsg";
        let handle = classes::argument(&args, FUNCTION)?;
        let Some(message) = args::array(&args, 1) else {
            return Ok(Value::Bool(false));
        };
        let mut parts = Vec::new();
        if let Some(value) = message.get(&ArrayKey::str("iov")) {
            let value = value.deref();
            let Value::Array(iov) = &*value else {
                return malformed(context, FUNCTION, " > iov", "expected an array here");
            };
            for value in iov.values() {
                parts.push(to_php_string(value, context.diagnostics())?);
            }
        }
        let iov: Vec<_> = parts
            .iter()
            .map(|bytes| IoSlice::new(bytes.as_bytes()))
            .collect();
        let connection = match data::connection(&handle, FUNCTION).await? {
            Ok(connection) => connection,
            Err(error) => {
                return failure(context, Some(&handle), FUNCTION, "Error in sendmsg", &error);
            }
        };
        let endpoint = match endpoint::parse(context, &connection, message).await? {
            Ok(endpoint) => endpoint,
            Err(error) => return malformed(context, FUNCTION, " > name", &error),
        };
        let rights = match control::send_descriptors(message).await {
            Ok(rights) => rights,
            Err(error) => return malformed(context, FUNCTION, " > control", &error),
        };
        let fds: Vec<_> = rights.iter().map(std::os::fd::AsFd::as_fd).collect();
        let mut space = vec![MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(fds.len()))];
        let mut ancillary = SendAncillaryBuffer::new(&mut space);
        if !fds.is_empty() {
            ancillary.push(rustix::net::SendAncillaryMessage::ScmRights(&fds));
        }
        let flags = args::int(&args, 2).unwrap_or_default() as i32;
        let blocking = handle.is_blocking() && flags & libc::MSG_DONTWAIT == 0;
        let flags = SendFlags::from_bits_truncate((flags | libc::MSG_NOSIGNAL) as _);
        let result = connection
            .operate(Interest::WRITABLE, blocking, |socket| {
                match &endpoint {
                    Some(endpoint::Destination::Ip(address)) => {
                        sendmsg_addr(socket, address, &iov, &mut ancillary, flags)
                    }
                    Some(endpoint::Destination::Unix(address)) => {
                        sendmsg_addr(socket, address, &iov, &mut ancillary, flags)
                    }
                    None => sendmsg(socket, &iov, &mut ancillary, flags),
                }
                .map_err(Into::into)
            })
            .await;
        match result {
            Ok(size) => Ok(Value::Int(size as i64)),
            Err(error) => failure(context, Some(&handle), FUNCTION, "Error in sendmsg", &error),
        }
    })
}

fn receive(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "socket_recvmsg";
        let handle = classes::argument(&args, FUNCTION)?;
        let Some(message) = args::array(&args, 1) else {
            return Ok(Value::Bool(false));
        };
        let Some(space) = message.get(&ArrayKey::str("controllen")) else {
            return malformed(context, FUNCTION, "", "The key 'controllen' is required");
        };
        let space = to_int(space, context.diagnostics())?;
        if space == 0 {
            return malformed(context, FUNCTION, " > controllen", "controllen cannot be 0");
        }
        if !(1..=65536).contains(&space) {
            return malformed(
                context,
                FUNCTION,
                " > controllen",
                "invalid control buffer length",
            );
        }
        let length = message
            .get(&ArrayKey::str("buffer_size"))
            .map(|length| to_int(length, context.diagnostics()))
            .transpose()?
            .unwrap_or(8192)
            .clamp(1, 16 * 1024 * 1024);
        let mut bytes = vec![0; length as usize];
        let mut control_space = vec![MaybeUninit::uninit(); space as usize];
        let mut ancillary = RecvAncillaryBuffer::new(&mut control_space);
        let connection = match data::connection(&handle, FUNCTION).await? {
            Ok(connection) => connection,
            Err(error) => {
                return failure(context, Some(&handle), FUNCTION, "Error in recvmsg", &error);
            }
        };
        let flags = args::int(&args, 2).unwrap_or_default() as i32;
        let blocking = handle.is_blocking() && flags & libc::MSG_DONTWAIT == 0;
        let flags = RecvFlags::from_bits_truncate(flags as _);
        let result = connection
            .operate(Interest::READABLE, blocking, |socket| {
                recvmsg(
                    socket,
                    &mut [IoSliceMut::new(&mut bytes)],
                    &mut ancillary,
                    flags,
                )
                .map_err(Into::into)
            })
            .await;
        match result {
            Ok(result) => {
                bytes.truncate(result.bytes);
                let mut fields = PhpArray::new();
                fields.insert(ArrayKey::str("name"), endpoint::received(result.address));
                fields.insert(
                    ArrayKey::str("control"),
                    Value::from(control::received(context, &mut ancillary)?),
                );
                fields.insert(
                    ArrayKey::str("iov"),
                    Value::from(PhpArray::from_values([Value::from(bytes)], 1)),
                );
                fields.insert(
                    ArrayKey::str("flags"),
                    Value::Int(i64::from(result.flags.bits())),
                );
                address::assign(&args, 1, Value::from(fields));
                Ok(Value::Int(result.bytes as i64))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                crate::status::set_error(context, Some(&handle), libc::EAGAIN);
                Ok(Value::Bool(false))
            }
            Err(error) => failure(context, Some(&handle), FUNCTION, "Error in recvmsg", &error),
        }
    })
}

fn malformed(context: &mut dyn Context, function: &str, path: &str, message: &str) -> NativeResult {
    context.warning(format!(
        "{function}(): error converting user data (path: msghdr{path}): {message}"
    ));
    Ok(Value::Bool(false))
}
