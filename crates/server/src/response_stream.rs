//! A response can start before its PHP request finishes.
//!
//! ```php
//! echo "working\n";
//! flush();
//! sleep(1);
//! echo "done\n";
//! ```

use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use tokio::sync::mpsc;

use crate::{Body, Response};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Waiting,
    Watching,
    Published,
    Disconnected,
}

pub(crate) struct ResponseSlot {
    phase: Cell<Phase>,
    response: Cell<Option<Box<Response>>>,
}

impl ResponseSlot {
    pub(crate) fn new() -> Self {
        Self {
            phase: Cell::new(Phase::Waiting),
            response: Cell::new(None),
        }
    }

    pub(crate) fn take(&self) -> Option<Response> {
        self.response.take().map(|response| *response)
    }

    /// PHP still gets to observe a disconnect at its next write.
    pub(crate) fn disconnect(&self) -> bool {
        if self.phase.get() == Phase::Watching {
            self.phase.set(Phase::Disconnected);
            true
        } else {
            false
        }
    }
}

/// Lets a request hand off its response while it keeps running.
pub struct ResponseWriter {
    slot: Rc<ResponseSlot>,
    head_only: bool,
}

impl ResponseWriter {
    pub(crate) fn new(slot: Rc<ResponseSlot>, head_only: bool) -> Self {
        Self { slot, head_only }
    }

    /// Keep the request alive after a disconnect, so the script can handle it.
    pub fn watch_disconnect(&self) {
        self.slot.phase.set(Phase::Watching);
    }

    pub fn is_disconnected(&self) -> bool {
        self.slot.phase.get() == Phase::Disconnected
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
        self.publish(response);
        sender
    }

    fn publish(&self, response: Response) -> bool {
        if matches!(self.slot.phase.get(), Phase::Waiting | Phase::Watching) {
            self.slot.phase.set(Phase::Published);
            self.slot.response.set(Some(Box::new(response)));
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
