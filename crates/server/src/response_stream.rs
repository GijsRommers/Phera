//! A response can start before its PHP request finishes.
//!
//! ```php
//! echo "working\n";
//! flush();
//! sleep(1);
//! echo "done\n";
//! ```

use std::cell::Cell;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use tokio::sync::{OwnedSemaphorePermit, mpsc};

use crate::response_task::ConnectionWork;
use crate::{Body, Response};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Waiting,
    Published,
    Disconnected,
}

pub(crate) struct ResponseSlot {
    phase: Cell<Phase>,
    generation: Cell<u64>,
    /// Buffered requests carry only the empty pointer.
    response: Cell<Option<Box<Response>>>,
}

impl ResponseSlot {
    pub(crate) fn new() -> Self {
        Self {
            phase: Cell::new(Phase::Waiting),
            generation: Cell::new(0),
            response: Cell::new(None),
        }
    }

    pub(crate) fn take(&self) -> Option<Response> {
        self.response.take().map(|response| *response)
    }

    pub(crate) fn begin(&self) -> u64 {
        self.phase.set(Phase::Waiting);
        self.response.set(None);
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        generation
    }

    /// PHP still gets to observe a disconnect at its next write.
    pub(crate) fn disconnect(&self) -> bool {
        if self.phase.get() == Phase::Waiting {
            self.phase.set(Phase::Disconnected);
            true
        } else {
            false
        }
    }
}

/// HTTP/1 has one service future at a time. It reuses its connection's
/// slot. An HTTP/2 stream owns a slot because it can run concurrently.
#[derive(Clone)]
pub(crate) struct ResponseExchange {
    pub(crate) connection: Rc<ConnectionWork>,
    stream: Option<Rc<Http2Exchange>>,
    generation: u64,
}

struct Http2Exchange {
    slot: ResponseSlot,
    _permit: OwnedSemaphorePermit,
}

impl ResponseExchange {
    pub(crate) fn new(connection: Rc<ConnectionWork>, version: crate::HttpVersion) -> Option<Self> {
        let (stream, generation) = if version == crate::HttpVersion::Http2 {
            let permit = connection.detached.http2_slot()?;
            (
                Some(Rc::new(Http2Exchange {
                    slot: ResponseSlot::new(),
                    _permit: permit,
                })),
                0,
            )
        } else {
            (None, connection.response.begin())
        };
        Some(Self {
            connection,
            stream,
            generation,
        })
    }

    pub(crate) fn slot(&self) -> &ResponseSlot {
        self.stream
            .as_ref()
            .map_or(&self.connection.response, |stream| &stream.slot)
    }
}

/// Lets a request hand off its response while it keeps running.
pub struct ResponseWriter {
    exchange: ResponseExchange,
    head_only: bool,
}

tokio::task_local! {
    static CURRENT_RESPONSE: Cell<Option<ResponseWriter>>;
}

impl ResponseWriter {
    pub(crate) fn new(exchange: ResponseExchange, head_only: bool) -> Self {
        Self {
            exchange,
            head_only,
        }
    }

    pub(crate) fn scope<F: Future>(self, request: F) -> impl Future<Output = F::Output> {
        CURRENT_RESPONSE.scope(Cell::new(Some(self)), request)
    }

    /// Claim the connection's writer on the first transport flush.
    pub fn take_current() -> Option<Self> {
        CURRENT_RESPONSE.try_with(Cell::take).ok().flatten()
    }

    pub fn is_disconnected(&self) -> bool {
        let slot = self.exchange.slot();
        slot.phase.get() == Phase::Disconnected || self.exchange.generation != slot.generation.get()
    }

    /// Send an entire response early, with its normal body length.
    pub fn finish(&self, response: Response) -> bool {
        self.publish(response)
    }

    /// Send the response head and start an unknown-length body.
    pub fn stream(&self, mut response: Response) -> Option<StreamWriter> {
        let sender =
            if !self.head_only && response.status >= 200 && !matches!(response.status, 204 | 304) {
                // The producer can be at most one chunk ahead of Hyper.
                let (sender, receiver) = mpsc::channel(1);
                let finished = Arc::new(AtomicBool::new(false));
                response.body = Body::Stream(StreamBody {
                    receiver,
                    finished: finished.clone(),
                });
                Some(StreamWriter { sender, finished })
            } else {
                None
            };
        if self.publish(response) { sender } else { None }
    }

    fn publish(&self, response: Response) -> bool {
        let slot = self.exchange.slot();
        if self.exchange.generation == slot.generation.get() && slot.phase.get() == Phase::Waiting {
            slot.phase.set(Phase::Published);
            slot.response.set(Some(Box::new(response)));
            true
        } else {
            false
        }
    }
}

/// Produces a streamed body. Cancelling the producer before it calls
/// `finish` makes the response fail instead of ending normally.
pub struct StreamWriter {
    sender: mpsc::Sender<Bytes>,
    finished: Arc<AtomicBool>,
}

impl StreamWriter {
    pub fn is_disconnected(&self) -> bool {
        self.sender.is_closed()
    }

    pub async fn write(&self, bytes: Vec<u8>) -> bool {
        const CHUNK_BYTES: usize = 64 * 1024;
        let mut bytes = Bytes::from(bytes);
        while !bytes.is_empty() {
            let chunk = bytes.split_to(bytes.len().min(CHUNK_BYTES));
            if self.sender.send(chunk).await.is_err() {
                return false;
            }
        }
        true
    }

    pub fn finish(self) {
        self.finished.store(true, Ordering::Release);
    }
}

/// The receiving end of a response whose length is not known in advance.
#[derive(Debug)]
pub struct StreamBody {
    receiver: mpsc::Receiver<Bytes>,
    finished: Arc<AtomicBool>,
}

impl StreamBody {
    pub(crate) fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<io::Result<Bytes>>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(None) if self.finished.load(Ordering::Acquire) => Poll::Ready(None),
            Poll::Ready(None) => Poll::Ready(Some(Err(io::Error::other(
                "streamed response ended before it completed",
            )))),
            Poll::Pending => Poll::Pending,
        }
    }
}
