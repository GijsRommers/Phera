//! The handler traits the PHP runtime plugs into.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use crate::request::Request;
use crate::response::Response;

/// Handles one request.
///
/// Implemented by the PHP runtime. Deliberately not `Send`: a handler lives
/// on a single worker thread, so it can hold `Rc` and `RefCell` state. The
/// returned future borrows the handler and is polled on that same thread.
pub trait Handler: 'static {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>>;
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
