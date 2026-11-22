//! `socket_recv($socket, $data, 4096, MSG_PEEK)` reads without consuming the message.

use std::{io, sync::Arc};

use nix::libc;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};
use rustix::net::{RecvFlags, recv, recvfrom};
use socket2::SockRef;
use tokio::io::Interest;

use crate::{ADDRESS, FLAGS, LENGTH, PORT, SOCKET, address, classes, failure, status};

const MAX_READ: usize = 16 * 1024 * 1024;
const DATA: Param = Param::new("data", ParamType::String);
const OUTPUT: Param = Param::new("data", ParamType::Mixed).by_ref();

pub(crate) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous(
            "socket_read",
            const {
                &[
                    SOCKET,
                    LENGTH,
                    Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(2)),
                ]
            },
            read,
        ),
        FunctionEntry::asynchronous(
            "socket_write",
            const { &[SOCKET, DATA, LENGTH.nullable().optional()] },
            write,
        ),
        FunctionEntry::asynchronous("socket_recv", &[SOCKET, OUTPUT, LENGTH, FLAGS], receive),
        FunctionEntry::asynchronous(
            "socket_recvfrom",
            const {
                &[
                    SOCKET,
                    OUTPUT,
                    LENGTH,
                    FLAGS,
                    Param::new("address", ParamType::Mixed).by_ref(),
                    Param::new("port", ParamType::Mixed).by_ref().optional(),
                ]
            },
            receive_from,
        ),
        FunctionEntry::asynchronous("socket_send", &[SOCKET, DATA, LENGTH, FLAGS], send),
        FunctionEntry::asynchronous(
            "socket_sendto",
            const {
                &[
                    SOCKET,
                    DATA,
                    LENGTH,
                    FLAGS,
                    ADDRESS,
                    PORT.nullable().optional(),
                ]
            },
            send_to,
        ),
    ] {
        registry.function(entry);
    }
}

fn read(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "socket_read";
        let handle = classes::argument(&args, function)?;
        let Some(length) = read_length(&args, 1) else {
            return Ok(Value::Bool(false));
        };
        let mode = args::int(&args, 2).unwrap_or(2);
        let connection = match connection(&handle, function).await? {
            Ok(connection) => connection,
            Err(error) => {
                return failure(
                    context,
                    Some(&handle),
                    function,
                    "unable to read from socket",
                    &error,
                );
            }
        };
        let mut bytes = vec![0; length];
        let mut position = 0;
        let result = loop {
            let target = if mode == 1 {
                &mut bytes[position..position + 1]
            } else {
                &mut bytes[..]
            };
            let received = connection
                .operate(Interest::READABLE, handle.is_blocking(), |socket| {
                    recv(socket, &mut *target, RecvFlags::empty())
                        .map(|(_, size)| size)
                        .map_err(Into::into)
                })
                .await;
            match received {
                Ok(0) => break Ok(position),
                Ok(size) => {
                    position += size;
                    if mode != 1
                        || position == length
                        || matches!(bytes[position - 1], b'\r' | b'\n')
                    {
                        break Ok(position);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock && position != 0 => {
                    break Ok(position);
                }
                Err(error) => break Err(error),
            }
        };
        match result {
            Ok(count) => {
                bytes.truncate(count);
                Ok(Value::from(bytes))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                status::set_error(context, Some(&handle), libc::EAGAIN);
                Ok(Value::Bool(false))
            }
            Err(error) => failure(
                context,
                Some(&handle),
                function,
                "unable to read from socket",
                &error,
            ),
        }
    })
}

fn receive(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(recv_arguments(context, args, false))
}

fn receive_from(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(recv_arguments(context, args, true))
}

async fn recv_arguments(context: &mut dyn Context, args: Vec<Value>, from: bool) -> NativeResult {
    let function = if from {
        "socket_recvfrom"
    } else {
        "socket_recv"
    };
    let handle = classes::argument(&args, function)?;
    let Some(length) = read_length(&args, 2) else {
        return Ok(Value::Bool(false));
    };
    let connection = match connection(&handle, function).await? {
        Ok(connection) => connection,
        Err(error) => {
            return failure(
                context,
                Some(&handle),
                function,
                "unable to receive from socket",
                &error,
            );
        }
    };
    let flags = args::int(&args, 3).unwrap_or_default() as i32;
    let recv_flags = RecvFlags::from_bits_truncate(flags as _);
    let blocking = handle.is_blocking() && flags & libc::MSG_DONTWAIT == 0;
    let mut bytes = vec![0; length];
    let result = connection
        .operate(recv_interest(flags), blocking, |socket| {
            if from {
                recvfrom(socket, bytes.as_mut_slice(), recv_flags)
                    .map(|(_, size, source)| (size, source))
                    .map_err(Into::into)
            } else {
                recv(socket, bytes.as_mut_slice(), recv_flags)
                    .map(|(_, size)| (size, None))
                    .map_err(Into::into)
            }
        })
        .await;
    match result {
        Ok((count, source)) => {
            bytes.truncate(count);
            if let Some(target) = args::reference(&args, 1) {
                *target.borrow_mut() = if !from && count == 0 {
                    Value::Null
                } else {
                    Value::from(bytes)
                };
            }
            if from {
                address::assign_sender(&args, source.as_ref(), 4, 5);
            }
            Ok(Value::Int(count as i64))
        }
        Err(error) => {
            if !from && let Some(target) = args::reference(&args, 1) {
                *target.borrow_mut() = Value::Null;
            }
            status::set_error(
                context,
                Some(&handle),
                error.raw_os_error().unwrap_or(libc::EINVAL),
            );
            Ok(Value::Bool(false))
        }
    }
}

fn read_length(args: &[Value], index: usize) -> Option<usize> {
    let length = args::int(args, index).unwrap_or_default();
    (length > 0).then(|| (length as u64).min(MAX_READ as u64) as usize)
}

fn recv_interest(flags: i32) -> Interest {
    #[cfg(target_os = "linux")]
    if flags & libc::MSG_OOB != 0 {
        return Interest::PRIORITY;
    }
    let _ = flags;
    Interest::READABLE
}

fn write(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(send_arguments(context, args, "socket_write", false))
}

fn send(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(send_arguments(context, args, "socket_send", false))
}

fn send_to(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(send_arguments(context, args, "socket_sendto", true))
}

async fn send_arguments(
    context: &mut dyn Context,
    args: Vec<Value>,
    function: &str,
    datagram: bool,
) -> NativeResult {
    let handle = classes::argument(&args, function)?;
    let bytes = args::bytes(&args, 1);
    let length = args::int(&args, 2).unwrap_or(bytes.len() as i64);
    let Ok(length) = usize::try_from(length) else {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #3 ($length) must be greater than or equal to 0"
        )));
    };
    let bytes = &bytes[..length.min(bytes.len())];
    let connection = match connection(&handle, function).await? {
        Ok(connection) => connection,
        Err(error) => {
            return failure(
                context,
                Some(&handle),
                function,
                "unable to write to socket",
                &error,
            );
        }
    };
    let flags = args::int(&args, 3).unwrap_or_default() as i32;
    let destination = if datagram {
        match address::destination(&connection, &args, 4, 5, function, true).await? {
            Ok(address) => Some(address),
            Err(error) => return address::address_failure(context, &handle, function, &error),
        }
    } else {
        None
    };
    let blocking = handle.is_blocking() && flags & libc::MSG_DONTWAIT == 0;
    let sent = connection
        .operate(Interest::WRITABLE, blocking, |socket| {
            let socket = SockRef::from(socket);
            let flags = flags | libc::MSG_NOSIGNAL;
            match &destination {
                Some(destination) => socket.send_to_with_flags(bytes, destination, flags),
                None => socket.send_with_flags(bytes, flags),
            }
        })
        .await;
    match sent {
        Ok(count) => Ok(Value::Int(count as i64)),
        Err(error) => failure(
            context,
            Some(&handle),
            function,
            "unable to write to socket",
            &error,
        ),
    }
}

pub(crate) async fn connection(
    handle: &classes::Handle,
    function: &str,
) -> Result<io::Result<Arc<phera_ext_streams::SocketConnection>>, NativeError> {
    Ok(handle.raw(function)?.connect().await)
}
