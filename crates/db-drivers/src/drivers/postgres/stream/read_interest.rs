//! Small sends normally complete before PostgreSQL replies. On kqueue,
//! keeping write interest registered can wake the worker for writable progress
//! before the response arrives.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream as StdTcpStream};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use tokio::io::{AsyncRead, AsyncWrite, Interest, ReadBuf, unix::AsyncFd};
use tokio::net::TcpStream;

const READ_CAPACITY: usize = 16 * 1024;

enum Registration {
    Read(AsyncFd<StdTcpStream>),
    ReadWrite(TcpStream),
}

pub(in super::super) struct Tcp {
    registration: Option<Registration>,
    // Keep initialized space for std's safe Read even when ReadBuf has none.
    read_buffer: Box<[u8]>,
    // Re-arm a pending reader when swapping the reactor registration drops its waker.
    pending_read_waker: Option<Waker>,
}

impl Tcp {
    pub(in super::super) fn new(stream: TcpStream) -> io::Result<Self> {
        Ok(Self {
            registration: Some(Registration::Read(register_read(stream.into_std()?)?)),
            read_buffer: vec![0; READ_CAPACITY].into_boxed_slice(),
            pending_read_waker: None,
        })
    }

    /// `into_inner` removes the old interest before Tokio registers both.
    fn enable_write_interest(&mut self) -> io::Result<()> {
        let Some(Registration::Read(socket)) = self.registration.take() else {
            return Err(disconnected());
        };
        self.registration = Some(Registration::ReadWrite(TcpStream::from_std(
            socket.into_inner(),
        )?));
        if let Some(waker) = self.pending_read_waker.take() {
            waker.wake();
        }
        Ok(())
    }

    fn finish_write(&mut self) -> io::Result<()> {
        if self.registration.is_none() {
            return Err(disconnected());
        }
        if matches!(self.registration, Some(Registration::ReadWrite(_))) {
            let Some(Registration::ReadWrite(socket)) = self.registration.take() else {
                return Err(disconnected());
            };
            self.registration = Some(Registration::Read(register_read(socket.into_std()?)?));
            if let Some(waker) = self.pending_read_waker.take() {
                waker.wake();
            }
        }
        Ok(())
    }
}

#[expect(
    deprecated,
    reason = "StdTcpStream owns a stable descriptor and this adapter never replaces it while registered"
)]
fn register_read(stream: StdTcpStream) -> io::Result<AsyncFd<StdTcpStream>> {
    AsyncFd::with_interest(stream, Interest::READABLE)
}

fn disconnected() -> io::Error {
    io::Error::new(ErrorKind::NotConnected, "PostgreSQL TCP socket is closed")
}

impl AsyncRead for Tcp {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let Self {
            registration,
            read_buffer,
            pending_read_waker,
        } = self.get_mut();
        match registration {
            Some(Registration::Read(socket)) => {
                let capacity = output.remaining().min(read_buffer.len());
                if capacity == 0 {
                    *pending_read_waker = None;
                    return Poll::Ready(Ok(()));
                }
                loop {
                    let mut ready = match socket.poll_read_ready(cx) {
                        Poll::Ready(Ok(ready)) => ready,
                        Poll::Ready(Err(error)) => {
                            *pending_read_waker = None;
                            return Poll::Ready(Err(error));
                        }
                        Poll::Pending => {
                            *pending_read_waker = Some(cx.waker().clone());
                            return Poll::Pending;
                        }
                    };
                    match socket.get_ref().read(&mut read_buffer[..capacity]) {
                        Ok(read) => {
                            // A short read drained this edge-triggered socket.
                            if 0 < read && read < capacity {
                                ready.clear_ready();
                            }
                            *pending_read_waker = None;
                            output.put_slice(&read_buffer[..read]);
                            return Poll::Ready(Ok(()));
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => ready.clear_ready(),
                        Err(error) => {
                            *pending_read_waker = None;
                            return Poll::Ready(Err(error));
                        }
                    }
                }
            }
            Some(Registration::ReadWrite(socket)) => {
                let poll = Pin::new(socket).poll_read(cx, output);
                if poll.is_pending() {
                    *pending_read_waker = Some(cx.waker().clone());
                } else {
                    *pending_read_waker = None;
                }
                poll
            }
            None => Poll::Ready(Err(disconnected())),
        }
    }
}

impl AsyncWrite for Tcp {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        input: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(Registration::Read(socket)) = &self.registration {
            match socket.get_ref().write(input) {
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    self.enable_write_interest()?
                }
                result => return Poll::Ready(result),
            }
        }
        match &mut self.registration {
            Some(Registration::ReadWrite(socket)) => Pin::new(socket).poll_write(cx, input),
            _ => Poll::Ready(Err(disconnected())),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        // The TCP socket has no userspace write buffer. Restore its idle interest.
        Poll::Ready(self.finish_write())
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.registration {
            Some(Registration::Read(socket)) => {
                Poll::Ready(socket.get_ref().shutdown(Shutdown::Write))
            }
            Some(Registration::ReadWrite(socket)) => Pin::new(socket).poll_shutdown(cx),
            None => Poll::Ready(Err(disconnected())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::Waker;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpSocket};
    use tokio::sync::Notify;

    use super::*;

    #[tokio::test]
    async fn slow_peer_forces_write_interest_then_returns_to_read_interest() -> io::Result<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let socket = TcpSocket::new_v4()?;
            socket.set_send_buffer_size(4096)?;
            let mut client = Tcp::new(socket.connect(listener.local_addr()?).await?)?;
            let (mut peer, _) = listener.accept().await?;
            let sql = format!("SELECT '{}'", "x".repeat(4 * 1024 * 1024));
            let mut payload = bytes::BytesMut::new();
            postgres_protocol::message::frontend::query(&sql, &mut payload)?;
            let mut written = 0;
            let mut cx = Context::from_waker(Waker::noop());
            while written < payload.len() {
                match Pin::new(&mut client).poll_write(&mut cx, &payload[written..]) {
                    Poll::Ready(Ok(0)) => return Err(io::Error::from(ErrorKind::WriteZero)),
                    Poll::Ready(result) => written += result?,
                    Poll::Pending => break,
                }
            }
            assert!(written < payload.len());
            assert!(matches!(
                client.registration,
                Some(Registration::ReadWrite(_))
            ));

            let length = payload.len();
            let read = async {
                let mut received = vec![0; length];
                peer.read_exact(&mut received).await?;
                peer.write_all(b"ready").await?;
                Ok::<_, io::Error>(received)
            };
            let write = async {
                client.write_all(&payload[written..]).await?;
                client.flush().await?;
                assert!(matches!(client.registration, Some(Registration::Read(_))));
                let mut response = [0; 5];
                client.read_exact(&mut response).await?;
                assert_eq!(&response, b"ready");
                Ok::<_, io::Error>(())
            };
            let (received, ()) = tokio::try_join!(read, write)?;
            assert_eq!(&received[..], &payload[..]);
            Ok(())
        })
        .await?
    }

    #[tokio::test]
    async fn pending_read_survives_write_interest_swaps() -> io::Result<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let socket = TcpSocket::new_v4()?;
            socket.set_send_buffer_size(4096)?;
            let client = Tcp::new(socket.connect(listener.local_addr()?).await?)?;
            let (mut peer, _) = listener.accept().await?;
            let (mut read_half, mut write_half) = tokio::io::split(client);
            let payload = vec![b'a'; 4 * 1024 * 1024];
            let flushed = Notify::new();
            let drain = Notify::new();

            let reader = async {
                let mut response = [0; 5];
                read_half.read_exact(&mut response).await?;
                assert_eq!(&response, b"ready");
                Ok::<_, io::Error>(())
            };
            let writer = async {
                // Let the reader register its waker on the initial Read registration.
                tokio::task::yield_now().await;
                drain.notify_one();
                write_half.write_all(&payload).await?;
                write_half.flush().await?;
                // Let the reader re-register on the restored Read registration before replying.
                tokio::task::yield_now().await;
                flushed.notify_one();
                Ok::<_, io::Error>(())
            };
            let peer_task = async {
                drain.notified().await;
                let mut received = vec![0; payload.len()];
                peer.read_exact(&mut received).await?;
                assert_eq!(received, payload);
                flushed.notified().await;
                peer.write_all(b"ready").await?;
                Ok::<_, io::Error>(())
            };

            tokio::try_join!(reader, writer, peer_task)?;
            Ok(())
        })
        .await?
    }
}
