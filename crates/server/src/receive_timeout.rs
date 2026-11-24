//! The receive clock begins with the HTTP request head and continues into
//! the body so a client cannot renew the base allowance with each read.

use std::future::Future;
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant as StdInstant};

use hyper::rt::{Sleep, Timer};
use hyper_util::rt::TokioTimer;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::Instant;

const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Hyper owns the header timer. Remember its start for the body reader.
#[derive(Clone)]
pub(crate) struct HeadTimer(Arc<Mutex<HeadStart>>);

struct HeadStart {
    accepted: Instant,
    current: Option<Instant>,
}

impl Default for HeadTimer {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl HeadTimer {
    pub(crate) fn new(accepted: Instant) -> Self {
        Self(Arc::new(Mutex::new(HeadStart {
            accepted,
            current: None,
        })))
    }

    pub(crate) fn started(&self) -> Instant {
        self.0
            .lock()
            .ok()
            .and_then(|start| start.current)
            .unwrap_or_else(Instant::now)
    }
}

impl Timer for HeadTimer {
    fn now(&self) -> StdInstant {
        let Ok(mut start) = self.0.lock() else {
            return StdInstant::now();
        };
        let now = start.current.map_or(start.accepted, |_| Instant::now());
        start.current = Some(now);
        now.into_std()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Sleep>> {
        TokioTimer::new().sleep(duration)
    }

    fn sleep_until(&self, deadline: StdInstant) -> Pin<Box<dyn Sleep>> {
        TokioTimer::new().sleep_until(deadline)
    }
}

pub(crate) fn body_time_credit(bytes: usize) -> Duration {
    const BYTES_PER_SECOND: f64 = 16.0 * 1024.0;
    Duration::from_secs_f64(bytes as f64 / BYTES_PER_SECOND)
}

/// Keep the socket on the connection's worker after Hyper rejects a partial
/// head. This lets the worker send a 408 when no HTTP service was called.
#[derive(Clone)]
pub(crate) struct TimeoutStream(Arc<Mutex<TcpStream>>);

impl TimeoutStream {
    pub(crate) fn new(stream: TcpStream) -> Self {
        Self(Arc::new(Mutex::new(stream)))
    }
}

impl AsyncRead for TimeoutStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Pin::new(&mut *guard).poll_read(cx, buf)
    }
}

impl AsyncWrite for TimeoutStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Pin::new(&mut *guard).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Pin::new(&mut *guard).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        let guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        guard.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Pin::new(&mut *guard).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Pin::new(&mut *guard).poll_shutdown(cx)
    }
}

/// Applies an initial read deadline while `hyper-util` inspects whether a
/// connection begins with the HTTP/2 prior-knowledge preface. Once HTTP/1.1 or
/// HTTP/2 is identified, this wrapper stops polling any timer and leaves
/// protocol timeouts to Hyper (`header_read_timeout` for HTTP/1, keep-alive
/// pings for HTTP/2).
pub(crate) struct PrefaceTimeoutStream {
    stream: TimeoutStream,
    deadline: Option<Pin<Box<tokio::time::Sleep>>>,
    matched: Arc<AtomicUsize>,
}

impl PrefaceTimeoutStream {
    pub(crate) fn new(stream: TimeoutStream, limit: Duration) -> (Self, Arc<AtomicUsize>) {
        let matched = Arc::new(AtomicUsize::new(0));
        let wrapped = Self {
            stream,
            deadline: Some(Box::pin(tokio::time::sleep(limit))),
            matched: matched.clone(),
        };
        (wrapped, matched)
    }
}

impl AsyncRead for PrefaceTimeoutStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let read = Pin::new(&mut self.stream).poll_read(cx, buf);
        let Some(sleep) = self.deadline.as_mut() else {
            return read;
        };
        match read {
            Poll::Ready(Ok(())) => {
                let bytes = &buf.filled()[before..];
                if bytes.is_empty() {
                    self.deadline = None;
                    return Poll::Ready(Ok(()));
                }
                let matched = self.matched.load(Ordering::Relaxed);
                let mut offset = 0usize;
                while matched + offset < H2_PREFACE.len() && offset < bytes.len() {
                    if bytes[offset] != H2_PREFACE[matched + offset] {
                        self.matched.store(0, Ordering::Relaxed);
                        self.deadline = None;
                        return Poll::Ready(Ok(()));
                    }
                    offset += 1;
                }
                let next_matched = matched + offset;
                self.matched.store(next_matched, Ordering::Relaxed);
                if next_matched >= H2_PREFACE.len() {
                    self.deadline = None;
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                self.deadline = None;
                Poll::Ready(Err(error))
            }
            Poll::Pending => {
                if sleep.as_mut().poll(cx).is_ready() {
                    self.deadline = None;
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "connection idle before protocol preface",
                    )))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl AsyncWrite for PrefaceTimeoutStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
