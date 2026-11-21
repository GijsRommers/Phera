//! `stream_socket_recvfrom($socket, 512, 0, $peer)` returns a datagram and its source.

use std::{io, net::SocketAddr};

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};
use rustix::net::{RecvFlags, recvfrom};
use tokio::io::Interest;

use super::{address::Address, functions::socket_arg};

pub(super) fn receive(
    context: &mut dyn Context,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_recvfrom";
        let length = args::int(&args, 1).unwrap_or_default();
        if length <= 0 {
            return Err(NativeError::value_error(
                "stream_socket_recvfrom(): Argument #2 ($length) must be greater than 0",
            ));
        }
        let Some(socket) = socket_arg(&args, function, "socket", context).await? else {
            return Ok(Value::Bool(false));
        };
        let flags = args::int(&args, 2).unwrap_or_default();
        let flags = RecvFlags::from_bits_truncate(flags as _);
        let mut buffer = vec![0; length.min(1024 * 1024) as usize];
        let result = socket
            .wait(socket.socket.wire.0.async_io(Interest::READABLE, |_| {
                recvfrom(socket.socket.descriptor(), buffer.as_mut_slice(), flags)
                    .map_err(Into::into)
            }))
            .await;
        match result {
            Ok((length, _, sender)) => {
                buffer.truncate(length);
                if let Some(target) = args::reference(&args, 3) {
                    let address = sender.and_then(|addr| SocketAddr::try_from(addr).ok());
                    *target.borrow_mut() =
                        address.map_or(Value::Null, |addr| Value::from(addr.to_string()));
                }
                Ok(Value::from(buffer))
            }
            Err(error) => Ok(warn(context, function, "recvfrom", &error)),
        }
    })
}

pub(super) fn send(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_sendto";
        let Some(socket) = socket_arg(&args, function, "socket", context).await? else {
            return Ok(Value::Bool(false));
        };
        let flags = args::int(&args, 2).unwrap_or_default() as i32;
        let bytes = args::bytes(&args, 1);
        let address = args::bytes(&args, 3);
        let destination = if address.is_empty() {
            None
        } else {
            let address = match Address::parse(socket.socket.transport, address) {
                Ok(address) => address,
                Err(message) => {
                    context.warning(format!("{function}(): {message}"));
                    return Ok(Value::Bool(false));
                }
            };
            match address.resolve().await {
                Ok(addresses) => addresses.into_iter().next(),
                Err(error) => return Ok(warn(context, function, "sendto", &error)),
            }
        };
        let result = socket
            .wait(socket.socket.wire.0.async_io(Interest::WRITABLE, |_| {
                let descriptor = socket.socket.sockref();
                match &destination {
                    Some(address) => descriptor.send_to_with_flags(bytes, address, flags),
                    None => descriptor.send_with_flags(bytes, flags),
                }
            }))
            .await;
        Ok(match result {
            Ok(length) => Value::Int(length as i64),
            Err(error) => warn(context, function, "sendto", &error),
        })
    })
}

fn warn(context: &mut dyn Context, function: &str, call: &str, error: &io::Error) -> Value {
    context.warning(format!(
        "{function}(): {call} failed: {}",
        crate::errors::reason(error)
    ));
    Value::Bool(false)
}
