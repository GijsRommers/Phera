//! `stream_socket_recvfrom($socket, 512, 0, $peer)` returns a datagram and its source.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};
use rustix::net::{RecvFlags, recvfrom};
use tokio::io::Interest;

use super::{
    address::{Address, Transport, received_from},
    functions::{socket_arg, socket_from_resource},
};

pub(super) fn receive(
    context: &mut dyn Context,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_socket_recvfrom";
        let resource = crate::resource::stream_argument(
            args::value(&args, 0), function, "#1 ($socket)",
        )?;
        if let Some(target) = args::reference(&args, 3) {
            *target.borrow_mut() = Value::Null;
        }
        let length = args::int(&args, 1).unwrap_or_default();
        if length <= 0 {
            return Err(NativeError::value_error(
                "stream_socket_recvfrom(): Argument #2 ($length) must be greater than 0",
            ));
        }
        let Some(socket) = socket_from_resource(resource, function, context).await else {
            return Ok(Value::Bool(false));
        };
        let flags = args::int(&args, 2).unwrap_or_default();
        let flags = RecvFlags::from_bits_truncate(flags as _);
        let mut buffer = vec![0; length.min(1024 * 1024) as usize];
        let mut receive = || {
            recvfrom(socket.socket.descriptor(), buffer.as_mut_slice(), flags).map_err(Into::into)
        };
        let result = if socket.blocked {
            socket
                .wait(
                    socket
                        .socket
                        .wire
                        .0
                        .async_io(read_interest(flags), |_| receive()),
                )
                .await
        } else {
            receive()
        };
        match result {
            Ok((length, _, sender)) => {
                buffer.truncate(length);
                if let Some(target) = args::reference(&args, 3) {
                    *target.borrow_mut() = sender
                        .and_then(received_from)
                        .map_or(Value::Null, Value::from);
                }
                Ok(Value::from(buffer))
            }
            Err(_) => Ok(Value::Bool(false)),
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
        let target = args::bytes(&args, 3);
        let destination = if target.is_empty() {
            None
        } else {
            let resolved = match Address::parse(Transport::Tcp, target) {
                Ok(address) => address.resolve().await.ok()
                    .and_then(|addresses| addresses.into_iter().next()),
                Err(_) => None,
            };
            let Some(address) = resolved else {
                context.warning(format!(
                    "{function}(): Failed to parse `{}' into a valid network address",
                    String::from_utf8_lossy(target),
                ));
                return Ok(Value::Bool(false));
            }
            Some(address)
        };
        let send = || {
            let descriptor = socket.socket.sockref();
            match &destination {
                Some(address) => descriptor.send_to_with_flags(bytes, address, flags),
                None => descriptor.send_with_flags(bytes, flags),
            }
        };
        let result = if socket.blocked {
            socket
                .wait(
                    socket
                        .socket
                        .wire
                        .0
                        .async_io(Interest::WRITABLE, |_| send()),
                )
                .await
        } else {
            send()
        };
        Ok(match result {
            Ok(length) => Value::Int(length as i64),
            Err(error) => {
                let message = crate::errors::reason(&error);
                context.warning(format!("{function}(): {message}\n"));
                Value::Int(-1)
            }
        })
    })
}

fn read_interest(flags: RecvFlags) -> Interest {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if flags.contains(RecvFlags::OOB) {
        return Interest::PRIORITY;
    }
    let _ = flags;
    Interest::READABLE
}

