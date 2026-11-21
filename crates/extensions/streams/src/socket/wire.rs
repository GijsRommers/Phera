//! `fread($socket, 10)` waits for the worker's I/O reactor.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use socket2::Socket;
use tokio::io::{AsyncRead, AsyncWrite, Interest, ReadBuf};
use tokio::net::{TcpListener, TcpStream, UdpSocket, UnixDatagram, UnixListener, UnixStream};

use super::address::Transport;

pub(crate) enum Reactor {
    Tcp(TcpStream),
    TcpListener(TcpListener),
    Udp(UdpSocket),
    Unix(UnixStream),
    UnixListener(UnixListener),
    UnixDatagram(UnixDatagram),
}

impl Reactor {
    pub(super) fn new(socket: Socket, kind: Transport, listener: bool) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        match (kind, listener) {
            (Transport::Tcp | Transport::Tls(_), true) => {
                TcpListener::from_std(socket.into()).map(Self::TcpListener)
            }
            (Transport::Tcp | Transport::Tls(_), false) => {
                TcpStream::from_std(socket.into()).map(Self::Tcp)
            }
            (Transport::Udp, _) => UdpSocket::from_std(socket.into()).map(Self::Udp),
            (Transport::Unix, true) => {
                let fd: OwnedFd = socket.into();
                UnixListener::from_std(fd.into()).map(Self::UnixListener)
            }
            (Transport::Unix, false) => {
                let fd: OwnedFd = socket.into();
                UnixStream::from_std(fd.into()).map(Self::Unix)
            }
            (Transport::UnixDatagram, _) => {
                let fd: OwnedFd = socket.into();
                UnixDatagram::from_std(fd.into()).map(Self::UnixDatagram)
            }
        }
    }

    pub(super) async fn accept(&self) -> io::Result<Self> {
        match self {
            Self::TcpListener(listener) => Ok(Self::Tcp(listener.accept().await?.0)),
            Self::UnixListener(listener) => Ok(Self::Unix(listener.accept().await?.0)),
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    pub(super) async fn async_io<T>(
        &self,
        interest: Interest,
        mut work: impl FnMut(&Self) -> io::Result<T>,
    ) -> io::Result<T> {
        match self {
            Self::Tcp(socket) => socket.async_io(interest, || work(self)).await,
            Self::Udp(socket) => socket.async_io(interest, || work(self)).await,
            Self::Unix(socket) => socket.async_io(interest, || work(self)).await,
            Self::UnixDatagram(socket) => socket.async_io(interest, || work(self)).await,
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    fn try_io<T>(
        &self,
        interest: Interest,
        work: impl FnOnce(&Self) -> io::Result<T>,
    ) -> io::Result<T> {
        match self {
            Self::Tcp(socket) => socket.try_io(interest, || work(self)),
            Self::Udp(socket) => socket.try_io(interest, || work(self)),
            Self::Unix(socket) => socket.try_io(interest, || work(self)),
            Self::UnixDatagram(socket) => socket.try_io(interest, || work(self)),
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    fn poll_read(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self {
            Self::Tcp(socket) => socket.poll_read_ready(cx),
            Self::Udp(socket) => socket.poll_recv_ready(cx),
            Self::Unix(socket) => socket.poll_read_ready(cx),
            Self::UnixDatagram(socket) => socket.poll_recv_ready(cx),
            _ => Poll::Ready(Err(io::ErrorKind::Unsupported.into())),
        }
    }

    fn poll_write(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self {
            Self::Tcp(socket) => socket.poll_write_ready(cx),
            Self::Udp(socket) => socket.poll_send_ready(cx),
            Self::Unix(socket) => socket.poll_write_ready(cx),
            Self::UnixDatagram(socket) => socket.poll_send_ready(cx),
            _ => Poll::Ready(Err(io::ErrorKind::Unsupported.into())),
        }
    }
}

impl AsFd for Reactor {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Tcp(socket) => socket.as_fd(),
            Self::TcpListener(socket) => socket.as_fd(),
            Self::Udp(socket) => socket.as_fd(),
            Self::Unix(socket) => socket.as_fd(),
            Self::UnixListener(socket) => socket.as_fd(),
            Self::UnixDatagram(socket) => socket.as_fd(),
        }
    }
}

impl AsRawFd for Reactor {
    fn as_raw_fd(&self) -> RawFd {
        self.as_fd().as_raw_fd()
    }
}

#[derive(Clone)]
pub(super) struct Wire(pub(super) Arc<Reactor>);

impl AsyncRead for Wire {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            std::task::ready!(self.0.poll_read(cx))?;
            match self.0.try_io(Interest::READABLE, |fd| {
                Ok(rustix::io::read(fd, buf.initialize_unfilled())?)
            }) {
                Ok(count) => {
                    buf.advance(count);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            std::task::ready!(self.0.poll_write(cx))?;
            match self.0.try_io(Interest::WRITABLE, |fd| {
                socket2::SockRef::from(fd).send(bytes)
            }) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                result => return Poll::Ready(result),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(socket2::SockRef::from(self.0.as_ref()).shutdown(std::net::Shutdown::Write))
    }
}
