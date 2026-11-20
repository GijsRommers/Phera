//! Drive a request until its response is ready. A script that calls
//! `fastcgi_finish_request(); do_more_work();` keeps running on the worker
//! after the client has received the response.

use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, watch};

use crate::response_stream::{ResponseSlot, ResponseWriter};
use crate::{Handler, Request, Response};

/// A detached request retains its connection's slot and participates in
/// graceful shutdown even after its socket is gone.
pub(crate) struct ConnectionWork {
    pub(crate) _slot: OwnedSemaphorePermit,
    pub(crate) detached: Rc<DetachedRequests>,
}

pub(crate) struct DetachedRequests {
    tasks: watch::Sender<()>,
    stopping: Cell<bool>,
}

impl DetachedRequests {
    pub(crate) fn new() -> Self {
        Self {
            tasks: watch::channel(()).0,
            stopping: Cell::new(false),
        }
    }

    pub(crate) async fn drain(&self) {
        self.tasks.closed().await;
    }

    pub(crate) fn stop(&self) {
        self.stopping.set(true);
    }
}

type ResponseFuture = Pin<Box<dyn Future<Output = Response>>>;

struct ResponseTask {
    future: Option<ResponseFuture>,
    slot: Rc<ResponseSlot>,
    connection: Rc<ConnectionWork>,
}

impl ResponseTask {
    fn detach(&mut self) {
        if let Some(future) = self.future.take() {
            let connection = self.connection.clone();
            let guard = connection.detached.tasks.subscribe();
            tokio::task::spawn_local(async move {
                let _ = future.await;
                drop((guard, connection));
            });
        }
    }
}

impl Drop for ResponseTask {
    fn drop(&mut self) {
        if self.future.is_some()
            && !self.connection.detached.stopping.get()
            && self.slot.disconnect()
        {
            self.detach();
        }
    }
}

pub(crate) async fn answer(
    handler: Rc<dyn Handler>,
    request: Request,
    time_limit: Option<Duration>,
    connection: Rc<ConnectionWork>,
) -> Response {
    let slot = Rc::new(ResponseSlot::new());
    let writer = ResponseWriter::new(slot.clone(), request.method == "HEAD");
    let future = Box::pin(async move {
        let run = handler.handle_with_response(request, writer);
        match time_limit {
            None => run.await,
            Some(limit) => match tokio::time::timeout(limit, run).await {
                Ok(response) => response,
                Err(_) => Response::text(504, "the request took too long\n"),
            },
        }
    });
    let mut task = ResponseTask {
        future: Some(future),
        slot,
        connection,
    };
    poll_fn(|cx| {
        let Some(future) = task.future.as_mut() else {
            return Poll::Ready(internal_error());
        };
        let polled = catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx)));
        match polled {
            Ok(Poll::Pending) => match task.slot.take() {
                Some(response) => {
                    task.detach();
                    Poll::Ready(response)
                }
                None => Poll::Pending,
            },
            Ok(Poll::Ready(response)) => {
                task.future = None;
                Poll::Ready(task.slot.take().unwrap_or(response))
            }
            Err(_) => {
                task.future = None;
                Poll::Ready(internal_error())
            }
        }
    })
    .await
}

fn internal_error() -> Response {
    Response::text(500, "internal server error\n")
}
