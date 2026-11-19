//! The receive clock begins with the HTTP request head and continues into
//! the body so a client cannot renew the base allowance with each read.

use std::cell::RefCell;
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
use tokio::time::Instant;

/// Hyper owns the header timer. Remember its start for the body reader.
#[derive(Clone, Default)]
pub(crate) struct HeadTimer(Arc<Mutex<Option<Instant>>>);

impl HeadTimer {
    pub(crate) fn started(&self) -> Instant {
        self.0
            .lock()
            .ok()
            .and_then(|start| *start)
            .unwrap_or_else(Instant::now)
    }
}

impl Timer for HeadTimer {
    fn now(&self) -> StdInstant {
        let now = Instant::now();
        if let Ok(mut start) = self.0.lock() {
            *start = Some(now);
        }
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
