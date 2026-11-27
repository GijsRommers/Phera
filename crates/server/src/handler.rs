//! The handler traits the PHP runtime plugs into.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use crate::request::Request;
use crate::response::Response;
use crate::websocket::{WebSocketEvent, WebSocketOutcome};

/// Handles one request.
///
/// Implemented by the PHP runtime. Deliberately not `Send`: a handler lives
/// on a single worker thread, so it can hold `Rc` and `RefCell` state. The
/// returned future borrows the handler and is polled on that same thread.
pub trait Handler: 'static {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>>;

    /// Handles one request with an owned `Rc<Self>`, returning a `'static`
    /// future so the server avoids double-boxing on the sync fast path.
    fn handle_owned(self: Rc<Self>, request: Request) -> Pin<Box<dyn Future<Output = Response>>> {
        Box::pin(async move { self.handle(request).await })
    }

    /// Runs once on the worker's thread before it accepts connections, for
    /// work no request should wait for (booting an application). Clients
    /// that connect meanwhile wait in the operating system's queue.
    fn start(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async {})
    }

    /// Runs one WebSocket lifecycle event (`Open`, `Message`, or `Close`) on a
    /// dedicated worker-local task. Events for one connection execute strictly
    /// sequentially in arrival order.
    fn handle_websocket(
        &self,
        _event: WebSocketEvent,
    ) -> Pin<Box<dyn Future<Output = WebSocketOutcome> + '_>> {
        Box::pin(async { WebSocketOutcome::default() })
    }

    /// Checks whether a browser `Origin` header is allowed to upgrade to a
    /// WebSocket. Handlers behind trusted reverse proxies override this using
    /// the proxy-resolved `HTTPS` flag and `Host`.
    fn websocket_origin_allowed(
        &self,
        request: &Request,
        allowed_origins: Option<&[String]>,
    ) -> bool {
        crate::websocket::handshake::validate_origin(
            request.header("origin"),
            false,
            request.header("host"),
            allowed_origins,
        )
    }
}

/// Creates a handler per worker thread.
///
/// `Send + Sync` because one instance is shared across threads, but
/// [`create`](HandlerFactory::create) is called once on each worker thread,
/// inside that worker's runtime, so the `Rc<dyn Handler>` it returns never
/// leaves that thread.
pub trait HandlerFactory: Send + Sync + 'static {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler>;
}
