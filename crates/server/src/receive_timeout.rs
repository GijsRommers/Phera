//! The receive clock begins with the HTTP request head and continues into
//! the body so a client cannot renew the base allowance with each read.

use std::cell::RefCell;
use std::future::Future;
use std::io::{self, IoSlice};
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant as StdInstant};

use hyper::rt::{Sleep, Timer};
use hyper_util::rt::TokioTimer;
use tokio::time::{Instant, Sleep as TokioSleep};

/// Hyper owns the header timer. Remember its start for the body reader.
#[derive(Clone)]
pub(crate) struct HeadTimer(Arc<Mutex<HeadStart>>);

struct HeadStart {
    accepted: Instant,
    current: Option<Instant>,
}

impl HeadTimer {
    pub(crate) fn new(accepted: Instant) -> Self {
        Self(Arc::new(Mutex::new(HeadStart {
            accepted,
            current: None,
        })))
    }

    pub(crate) fn has_started(&self) -> bool {
        self.0.lock().is_ok_and(|start| start.current.is_some())
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
pub(crate) struct TimeoutStream(Rc<RefCell<TcpStream>>);

impl TimeoutStream {
    pub(crate) fn new(stream: TcpStream) -> Self {
        Self(Rc::new(RefCell::new(stream)))
    }
}

impl AsyncRead for TimeoutStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0.borrow_mut()).poll_read(cx, buf)
    }
}

impl AsyncWrite for TimeoutStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.0.borrow_mut()).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.0.borrow_mut()).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.borrow().is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0.borrow_mut()).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0.borrow_mut()).poll_shutdown(cx)
    }
}

/// Hyper starts its HTTP/1 head timer after it identifies the protocol. Give
/// the protocol preface its own deadline so a silent peer still times out.
pub(crate) struct PrefaceTimeout {
    stream: TimeoutStream,
    probe: Option<Preface>,
}

struct Preface {
    deadline: Instant,
    delay: Option<Pin<Box<TokioSleep>>>,
    matched: usize,
}

impl PrefaceTimeout {
    pub(crate) fn new(stream: TimeoutStream, deadline: Instant) -> Self {
        Self {
            stream,
            probe: Some(Preface {
                deadline,
                delay: None,
                matched: 0,
            }),
        }
    }
}

impl AsyncRead for PrefaceTimeout {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let original_len = buffer.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buffer);
        let Some(probe) = &mut self.probe else {
            return result;
        };
        match &result {
            Poll::Pending => {
                let delay = probe
                    .delay
                    .get_or_insert_with(|| Box::pin(tokio::time::sleep_until(probe.deadline)));
                if delay.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
                }
            }
            Poll::Ready(Ok(())) => {
                const HTTP2: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
                let bytes = &buffer.filled()[original_len..];
                let rest = &HTTP2[probe.matched..];
                if bytes.is_empty() || bytes.len() >= rest.len() || !rest.starts_with(bytes) {
                    self.probe = None;
                } else {
                    probe.matched += bytes.len();
                }
            }
            Poll::Ready(Err(_)) => {}
        }
        result
    }
}

impl AsyncWrite for PrefaceTimeout {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buffer)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffers: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, buffers)
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
