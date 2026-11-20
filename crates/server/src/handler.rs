//! The handler traits the PHP runtime plugs into.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use crate::ResponseWriter;
use crate::request::Request;
use crate::response::Response;

/// Handles one request.
///
/// Implemented by the PHP runtime. Deliberately not `Send`: a handler lives
/// on a single worker thread, so it can hold `Rc` and `RefCell` state. The
/// returned future borrows the handler and is polled on that same thread.
pub trait Handler: 'static {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>>;

    /// A handler can publish headers or the whole response before it returns.
    fn handle_with_response(
        &self,
        request: Request,
        _writer: ResponseWriter,
    ) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        self.handle(request)
    }

    /// Runs once on the worker's thread before it accepts connections, for
    /// work no request should wait for (booting an application). Clients
    /// that connect meanwhile wait in the operating system's queue.
    fn start(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async {})
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
