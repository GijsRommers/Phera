//! `fgets($socket)` buffers through the newline and leaves later bytes for the next read.

use std::io;
use std::sync::atomic::Ordering;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{Connection, Io, tls::State};

impl Io {
    pub(crate) async fn read(
        self,
        length: Option<u64>,
        ending: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        let limit = length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(usize::MAX);
        if self.socket.listener || limit == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = self.socket.input.lock().await;
        let mut output = Vec::new();
        let chunk = self.chunk.clamp(1, 64 * 1024);
        loop {
            let available = buffer.len().min(limit - output.len());
            for _ in 0..available {
                if let Some(byte) = buffer.pop_front() {
                    output.push(byte);
                }
                if let Some(ending) = ending.filter(|ending| !ending.is_empty())
                    && output.ends_with(ending)
                {
                    self.socket.unread.store(buffer.len(), Ordering::Relaxed);
                    return Ok(output);
                }
            }
            self.socket.unread.store(buffer.len(), Ordering::Relaxed);
            if output.len() == limit || self.socket.eof() {
                return Ok(output);
            }
            if length.is_some() && ending.is_none() && !output.is_empty() {
                return Ok(output);
            }
            let result = {
                let mut bytes = vec![0; chunk];
                let result = self
                    .wait(self.socket.receive(&mut bytes, self.blocked))
                    .await;
                match result {
                    Ok(0) => {
                        if !self.socket.transport.datagram() {
                            self.socket.eof.store(true, Ordering::Relaxed);
                        }
                        return Ok(output);
                    }
                    Ok(read) => {
                        buffer.extend(&bytes[..read]);
                        self.socket.unread.store(buffer.len(), Ordering::Relaxed);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            };
            match result {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock || !output.is_empty() => {
                    return Ok(output);
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(crate) async fn write(self, bytes: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < bytes.len() {
            match self
                .wait(self.socket.send(&bytes[written..], self.blocked))
                .await
            {
                Ok(0) => break,
                Ok(count) => written += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock || written > 0 => break,
                Err(error) => return Err(error),
            }
            if !self.blocked || self.socket.transport.datagram() {
                break;
            }
        }
        Ok(written)
    }

    pub(super) async fn wait<T>(
        &self,
        operation: impl std::future::Future<Output = io::Result<T>>,
    ) -> io::Result<T> {
        if !self.blocked {
            return tokio::time::timeout(std::time::Duration::ZERO, operation)
                .await
                .unwrap_or_else(|_| Err(io::ErrorKind::WouldBlock.into()));
        }
        let Some(timeout) = self.timeout else {
            return operation.await;
        };
        self.socket.timed_out.store(false, Ordering::Relaxed);
        match tokio::time::timeout(timeout, operation).await {
            Ok(result) => result,
            Err(_) => {
                self.socket.timed_out.store(true, Ordering::Relaxed);
                Err(io::ErrorKind::TimedOut.into())
            }
        }
    }
}

impl Connection {
    async fn receive(&self, target: &mut [u8], blocked: bool) -> io::Result<usize> {
        let mut secure = self.secure.lock().await;
        match &mut *secure {
            State::Established(stream) => stream.read(target).await,
            State::Plain if blocked => {
                drop(secure);
                self.wire.clone().read(target).await
            }
            State::Plain => Ok(rustix::io::read(self.descriptor(), target)?),
            State::Negotiating(_) => Err(io::ErrorKind::WouldBlock.into()),
            State::Failed => Err(io::ErrorKind::NotConnected.into()),
        }
    }

    async fn send(&self, bytes: &[u8], blocked: bool) -> io::Result<usize> {
        let mut secure = self.secure.lock().await;
        match &mut *secure {
            State::Established(stream) => {
                let written = stream.write(bytes).await?;
                stream.flush().await?;
                Ok(written)
            }
            State::Plain if blocked => {
                drop(secure);
                self.wire.clone().write(bytes).await
            }
            State::Plain => self.sockref().send(bytes),
            State::Negotiating(_) => Err(io::ErrorKind::WouldBlock.into()),
            State::Failed => Err(io::ErrorKind::NotConnected.into()),
        }
    }
}
