//! `socket_export_stream($socket)` shares the descriptor registered with the worker.

use std::{
    io,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    sync::{Arc, atomic::Ordering},
};

use nix::libc;
use phera_extension_api::{Context, NativeResult, Value};
use socket2::{Domain, SockAddr, SockRef, Socket as Descriptor, Type};
use tokio::io::Interest;

use super::{Connection, Socket, address::Transport};
use crate::{
    resource::with_stream,
    stream::{Backing, Stream, Wrapper},
};

impl Socket {
    pub fn register_descriptor(
        descriptor: Descriptor,
        domain: Domain,
        kind: Type,
        listener: bool,
    ) -> io::Result<Self> {
        let transport = match (domain, kind) {
            (Domain::UNIX, Type::DGRAM) => Transport::UnixDatagram,
            (Domain::UNIX, _) => Transport::Unix,
            (_, Type::DGRAM) => Transport::Udp,
            _ => Transport::Tcp,
        };
        Ok(Self::live(Connection::new(
            descriptor, transport, listener, true,
        )?))
    }
}

impl AsFd for Connection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.descriptor().as_fd()
    }
}

impl Connection {
    pub async fn operate<T>(
        &self,
        interest: Interest,
        blocking: bool,
        mut operation: impl FnMut(&Self) -> io::Result<T>,
    ) -> io::Result<T> {
        let result = operation(self);
        if !blocking || !matches!(&result, Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        {
            return result;
        }
        let wait = self.descriptor().async_io(interest, |_| operation(self));
        let socket = self.sockref();
        let timeout = if interest == Interest::WRITABLE {
            socket.write_timeout()?
        } else {
            socket.read_timeout()?
        };
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, wait)
                .await
                .unwrap_or_else(|_| Err(io::Error::from_raw_os_error(libc::EAGAIN))),
            None => wait.await,
        }
    }

    pub async fn connect_descriptor(&self, address: &SockAddr, blocking: bool) -> io::Result<()> {
        match self.sockref().connect(address) {
            Err(error)
                if blocking
                    && matches!(
                        error.raw_os_error(),
                        Some(libc::EINPROGRESS | libc::EALREADY)
                    ) =>
            {
                self.descriptor()
                    .async_io(Interest::WRITABLE, |_| {
                        match self.sockref().take_error()? {
                            Some(error) => Err(error),
                            None => Ok(()),
                        }
                    })
                    .await
            }
            result => result,
        }
    }

    pub fn listen_descriptor(&self, backlog: i32) -> io::Result<()> {
        self.sockref().listen(backlog)?;
        self.listener.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub async fn accept_descriptor(&self, blocking: bool) -> io::Result<Socket> {
        if blocking {
            let accept = self.descriptor().accept();
            let reactor = match self.sockref().read_timeout()? {
                Some(timeout) => tokio::time::timeout(timeout, accept)
                    .await
                    .unwrap_or_else(|_| Err(io::Error::from_raw_os_error(libc::EAGAIN))),
                None => accept.await,
            }?;
            return Ok(Socket::live(Connection::with_reactor(
                reactor,
                self.transport,
                false,
                true,
            )));
        }
        let (peer, address) = self
            .operate(Interest::READABLE, false, |socket| {
                SockRef::from(socket).accept()
            })
            .await?;
        let kind = peer.r#type()?;
        Socket::register_descriptor(peer, address.domain(), kind, false)
    }
}

pub fn import_socket_stream(stream: &Value) -> Option<Arc<Socket>> {
    let Value::Resource(resource) = &*stream.deref() else {
        return None;
    };
    with_stream(resource, |stream| match &stream.backing {
        Backing::Socket(socket) => Some(socket.clone()),
        _ => None,
    })
    .ok()?
}

pub fn export_socket_stream(
    context: &mut dyn Context,
    socket: Arc<Socket>,
    blocking: bool,
) -> Value {
    let mut stream = Stream::new(Backing::Socket(socket), Wrapper::Socket, b"r+", b"");
    stream.blocked = blocking;
    crate::request::new_resource(context.state(), "stream", stream)
}

pub fn socket_stream_blocking(stream: &Value) -> Option<bool> {
    let Value::Resource(resource) = &*stream.deref() else {
        return None;
    };
    with_stream(resource, |stream| stream.blocked).ok()
}

pub fn set_socket_stream_blocking(stream: &Value, blocking: bool) {
    if let Value::Resource(resource) = &*stream.deref() {
        let _ = with_stream(resource, |stream| stream.blocked = blocking);
    }
}

pub async fn close_socket_stream(context: &mut dyn Context, stream: Value) -> NativeResult {
    if import_socket_stream(&stream).is_some() {
        crate::handles::close_arguments(context, vec![stream], "socket_close").await?;
    }
    Ok(Value::Null)
}

pub fn received_file_stream(context: &mut dyn Context, fd: OwnedFd) -> io::Result<Value> {
    use rustix::fs::{OFlags, fcntl_getfl};

    let access = fcntl_getfl(&fd)? & OFlags::ACCMODE;
    let mode = match access {
        OFlags::WRONLY => b"w".as_slice(),
        OFlags::RDWR => b"r+",
        _ => b"r",
    };
    let stream = Stream::file(fd.into(), Wrapper::File, mode, b"");
    Ok(crate::request::new_resource(
        context.state(),
        "stream",
        stream,
    ))
}
