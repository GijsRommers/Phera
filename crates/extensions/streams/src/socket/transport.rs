//! `stream_socket_server('tcp://127.0.0.1:0')` binds a reactor-owned socket.

use std::io;
use std::sync::Arc;

use socket2::{SockAddr, Socket as Descriptor};
use tokio::io::Interest;

use super::Connection;
use super::address::{Address, Transport};

pub(super) async fn connect(
    address: &Address<'_>,
    source: Option<&[u8]>,
    asynchronous: bool,
) -> io::Result<Arc<Connection>> {
    let source = source
        .filter(|source| !source.is_empty() && !address.transport.unix())
        .map(|source| super::address::source(source))
        .transpose()?
        .map(SockAddr::from);
    let mut last = io::ErrorKind::AddrNotAvailable.into();
    for destination in address.resolve().await? {
        match connect_addr(
            address.transport,
            &destination,
            source.as_ref(),
            asynchronous,
        )
        .await
        {
            Ok(socket) => return Ok(socket),
            Err(error) => last = error,
        }
    }
    Err(last)
}

async fn connect_addr(
    transport: Transport,
    destination: &SockAddr,
    source: Option<&SockAddr>,
    asynchronous: bool,
) -> io::Result<Arc<Connection>> {
    let descriptor = Descriptor::new(destination.domain(), transport.socket_type(), None)?;
    descriptor.set_nonblocking(true)?;
    if let Some(source) = source
        && source.family() == destination.family()
    {
        descriptor.bind(source)?;
    }
    let pending = match descriptor.connect(destination) {
        Ok(()) => false,
        Err(error)
            if error.kind() == io::ErrorKind::WouldBlock
                || error.raw_os_error() == Some(nix::libc::EINPROGRESS) =>
        {
            true
        }
        Err(error) => return Err(error),
    };
    let socket = Connection::new(descriptor, transport, false, false)?;
    if pending && !asynchronous {
        socket
            .wire
            .0
            .async_io(Interest::WRITABLE, |_| {
                match socket.sockref().take_error()? {
                    Some(error) => Err(error),
                    None => Ok(()),
                }
            })
            .await?;
    }
    Ok(socket)
}

pub(super) async fn bind(
    address: &Address<'_>,
    should_bind: bool,
    should_listen: bool,
) -> io::Result<Arc<Connection>> {
    let mut last = io::ErrorKind::AddrNotAvailable.into();
    for local in address.resolve().await? {
        match bind_addr(address.transport, &local, should_bind, should_listen) {
            Ok(socket) => return Ok(socket),
            Err(error) => last = error,
        }
    }
    Err(last)
}

fn bind_addr(
    transport: Transport,
    local: &SockAddr,
    should_bind: bool,
    should_listen: bool,
) -> io::Result<Arc<Connection>> {
    let descriptor = Descriptor::new(local.domain(), transport.socket_type(), None)?;
    if should_bind {
        if !transport.unix() {
            descriptor.set_reuse_address(true)?;
        }
        descriptor.bind(local)?;
    }
    let listener = !transport.datagram() && should_listen;
    if listener {
        descriptor.listen(128)?;
    }
    Connection::new(descriptor, transport, listener, false)
}

pub(super) async fn accept(socket: &Connection) -> io::Result<(Arc<Connection>, SockAddr)> {
    let reactor = socket.wire.0.accept().await?;
    let address = socket2::SockRef::from(&reactor).peer_addr()?;
    Ok((
        Connection::with_reactor(reactor, socket.transport, false, false),
        address,
    ))
}
