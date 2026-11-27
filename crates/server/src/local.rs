//! Serving on a thread the caller already runs, for hosts that keep their
//! own state next to the server: `Phera\Http\Server` keeps one PHP VM per
//! worker thread, alive between requests.
//!
//! ```text
//! let first = bind_worker_listener(addr, true)?;  // the port is known now
//! let second = listener_for_next_worker(&first)?; // one per extra worker
//! // on each worker thread, inside a LocalSet:
//! serve_local(0, first, handler, config, shutdown).await;
//! ```
//!
//! [`Server`](crate::Server) owns its threads and builds each handler
//! through a [`HandlerFactory`](crate::HandlerFactory); these functions
//! leave threads, runtimes and handlers to the caller and run the same
//! connection code (HTTP/1.1 and HTTP/2, the limits of [`ServerConfig`],
//! graceful shutdown).

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::watch;

use crate::config::ServerConfig;
use crate::handler::Handler;
use crate::websocket::WebSocketHub;

/// Binds the listening socket of the first worker; `shared` when more
/// workers will listen on the port, see [`listener_for_next_worker`].
pub fn bind_worker_listener(addr: SocketAddr, shared: bool) -> io::Result<StdListener> {
    crate::worker::bind_worker_listener(addr, shared)
}

/// A listening socket for one more worker on the port of `first`: its own
/// `SO_REUSEPORT` socket on Linux, a duplicate of `first` elsewhere.
pub fn listener_for_next_worker(first: &StdListener) -> io::Result<StdListener> {
    crate::worker::listener_for_next_worker(first)
}

/// Serves connections from `listener` with `handler` on the current
/// thread, until `shutdown` turns true; then lets in-flight requests finish
/// up to `config.shutdown_timeout`. Must run inside a Tokio `LocalSet`:
/// every connection is a `spawn_local` task. `index` names the worker in
/// log lines.
pub async fn serve_local(
    index: usize,
    listener: StdListener,
    handler: Rc<dyn Handler>,
    config: ServerConfig,
    shutdown: watch::Receiver<bool>,
) {
    let ws_hub = Arc::new(WebSocketHub::new(
        config.websocket.send_queue_len,
        config.websocket.send_queue_bytes,
    ));
    crate::worker::serve_direct(index, listener, handler, ws_hub, config, shutdown).await;
}

/// Sets `shutdown` to true on the first SIGINT or SIGTERM, from a thread of
/// its own.
pub fn shutdown_on_signal(shutdown: Arc<watch::Sender<bool>>) {
    crate::shutdown::spawn_shared_signal_watcher(shutdown);
}
