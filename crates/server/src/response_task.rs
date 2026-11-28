//! Drive a request until its response is ready. A script that calls
//! `fastcgi_finish_request(); do_more_work();` keeps running on the worker
//! after the client has received the response.

use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};

use crate::response_stream::{ResponseExchange, ResponseSlot, ResponseWriter};
use crate::worker::ConnectionSlot;
use crate::{Handler, Request, Response};

/// A detached request retains its connection's slot and participates in
/// graceful shutdown even after its socket is gone.
pub(crate) struct ConnectionWork {
    pub(crate) tcp_slot: RefCell<Option<ConnectionSlot>>,
    pub(crate) detached: Rc<DetachedRequests>,
    pub(crate) response: ResponseSlot,
}

pub(crate) struct DetachedRequests {
    tasks: watch::Sender<()>,
    stopping: Cell<bool>,
    http2_requests: Arc<Semaphore>,
}

impl DetachedRequests {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            tasks: watch::channel(()).0,
            stopping: Cell::new(false),
            http2_requests: Arc::new(Semaphore::new(limit)),
        }
    }

    pub(crate) fn http2_slot(&self) -> Option<OwnedSemaphorePermit> {
        self.http2_requests.clone().try_acquire_owned().ok()
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
    exchange: ResponseExchange,
}

impl ResponseTask {
    fn detach(&mut self) {
        if let Some(future) = self.future.take() {
            let exchange = self.exchange.clone();
            let guard = exchange.connection.detached.tasks.subscribe();
            tokio::task::spawn_local(async move {
                let _ = future.await;
                drop((guard, exchange));
            });
        }
    }
}

impl Drop for ResponseTask {
    fn drop(&mut self) {
        if self.future.is_some()
            && !self.exchange.connection.detached.stopping.get()
            && self.exchange.slot().disconnect()
        {
            self.detach();
        }
    }
}

pub(crate) fn answer(
    handler: Rc<dyn Handler>,
    request: Request,
    time_limit: Option<Duration>,
    connection: Rc<ConnectionWork>,
) -> impl Future<Output = Response> {
    let exchange = ResponseExchange::new(connection, request.version);
    let head = request.method == "HEAD";
    // Consume `request` (~224 bytes) synchronously before returning the future
    // so `Request` never inflates `serve_connection`'s idle task footprint.
    let handler_fut = exchange.as_ref().map(|_| handler.handle_owned(request));
    async move {
        let (Some(exchange), Some(mut handler_fut)) = (exchange, handler_fut) else {
            return Response::text(503, "too many requests in flight\n");
        };
        let writer_exchange = exchange.clone();
        let mut writer_opt = Some(ResponseWriter::new(writer_exchange, head));
        // Fast path: poll once on the stack with `sync_scope` before arming a
        // Tokio timer, wrapping in a second `Box::pin`, or building a ResponseTask.
        // Sync handlers (/plaintext, /json, in-memory SQLite) complete here with
        // zero outer future allocation and zero timer-wheel overhead.
        let first = poll_fn(|cx| {
            let polled = ResponseWriter::sync_scope(&mut writer_opt, || {
                catch_unwind(AssertUnwindSafe(|| handler_fut.as_mut().poll(cx)))
            });
            Poll::Ready(polled)
        })
        .await;
        match first {
            Ok(Poll::Ready(response)) => exchange.slot().take().unwrap_or(response),
            Err(_) => internal_error(),
            Ok(Poll::Pending) => {
                finish_pending(exchange, writer_opt, handler_fut, time_limit).await
            }
        }
    }
}

/// Cold async continuation for requests that do not finish on their first poll.
/// Boxed off `answer` so `ResponseTask` and timeout state never bloat idle
/// keep-alive connections' `serve_connection` futures.
#[cold]
fn finish_pending(
    exchange: ResponseExchange,
    writer_opt: Option<ResponseWriter>,
    handler_fut: ResponseFuture,
    time_limit: Option<Duration>,
) -> ResponseFuture {
    Box::pin(async move {
        let run: ResponseFuture = Box::pin(ResponseWriter::scope_opt(writer_opt, handler_fut));
        if let Some(response) = exchange.slot().take() {
            let guard = exchange.connection.detached.tasks.subscribe();
            let future: ResponseFuture = match time_limit {
                None => run,
                Some(limit) => Box::pin(async move {
                    match tokio::time::timeout(limit, run).await {
                        Ok(response) => response,
                        Err(_) => Response::text(504, "the request took too long\n"),
                    }
                }),
            };
            tokio::task::spawn_local(async move {
                let _ = future.await;
                drop((guard, exchange));
            });
            return response;
        }
        let future: ResponseFuture = match time_limit {
            None => run,
            Some(limit) => Box::pin(async move {
                match tokio::time::timeout(limit, run).await {
                    Ok(response) => response,
                    Err(_) => Response::text(504, "the request took too long\n"),
                }
            }),
        };
        let mut task = ResponseTask {
            future: Some(future),
            exchange,
        };
        poll_fn(|cx| {
            let Some(future) = task.future.as_mut() else {
                return Poll::Ready(internal_error());
            };
            let polled = catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx)));
            match polled {
                Ok(Poll::Pending) => match task.exchange.slot().take() {
                    Some(response) => {
                        task.detach();
                        Poll::Ready(response)
                    }
                    None => Poll::Pending,
                },
                Ok(Poll::Ready(response)) => {
                    task.future = None;
                    Poll::Ready(task.exchange.slot().take().unwrap_or(response))
                }
                Err(_) => {
                    task.future = None;
                    Poll::Ready(internal_error())
                }
            }
        })
        .await
    })
}

fn internal_error() -> Response {
    Response::text(500, "internal server error\n")
}
