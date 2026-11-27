//! `Phera\Http`: an HTTP server built into the runtime, in the style of Go's
//! `net/http` and `Bun.serve`, for scripts run with `phera app.php`.
//!
//! ```php
//! use Phera\Http\{Server, Request, Response};
//!
//! $server = new Server('127.0.0.1:8080', workers: 4);
//! $server->onWorkerStart(function (int $worker) use (&$db) {
//!     $db = new PDO('sqlite:app.db');            // once per worker
//! });
//! $server->get('/plaintext', fn() => 'Hello, World!');
//! $server->get('/users/{id}', fn(Request $r) => Response::json(['id' => $r->param('id')]));
//! $server->listen();                             // until SIGINT or SIGTERM
//! ```
//!
//! How it runs:
//!
//! - Every worker thread keeps one PHP VM alive for as long as the server
//!   runs. A request does not boot anything: the router (a flat radix tree,
//!   see [`router`]) picks the handler, and the worker calls it on its VM
//!   with a `Request` object over the request's own bytes. There is no
//!   `$_SERVER`, `$_GET` or output buffer in between; a handler returns a
//!   `Response` (or a string or an array) and that is what is sent.
//! - Worker 0 is the VM that ran the script and called `listen()`. Every
//!   other worker is a VM of its own on its own thread, which runs the same
//!   script from the top up to its `listen()` call (quietly: its output up to
//!   there repeats worker 0's), so it has the same routes and closures. No
//!   PHP value is ever shared between threads.
//! - Each worker accepts on its own socket: on Linux the workers' sockets
//!   share the port through `SO_REUSEPORT`, elsewhere they share one socket.
//!   The HTTP/1.1 and HTTP/2 connection code is `phera-server`'s.
//! - Requests on one worker run one at a time on its VM, in arrival order;
//!   while one waits for I/O the worker keeps reading and queueing others.
//!   An uncaught exception in a handler is reported as PHP reports an
//!   uncaught one, the request is answered 500, and the worker goes on.

mod dispatch;
mod request;
mod response;
mod router;
mod server;
mod worker;

use phera_extension_api::Registry;

pub(crate) use worker::{LaunchSlot, ScriptLaunch, WorkerOutput, WorkerRole, WorkerRoleSlot};

pub(crate) const SERVER_CLASS: &str = "Phera\\Http\\Server";
pub(crate) const REQUEST_CLASS: &str = "Phera\\Http\\Request";
pub(crate) const RESPONSE_CLASS: &str = "Phera\\Http\\Response";

/// Registers the `Phera\Http` classes.
pub(crate) fn register(registry: &mut Registry) {
    // What a command-line VM knows about starting workers is no state a
    // copy of the VM may share.
    registry.fork_state::<LaunchSlot>(phera_extension_api::fresh_state);
    registry.fork_state::<WorkerRoleSlot>(phera_extension_api::fresh_state);
    request::register(registry);
    response::register(registry);
    server::register(registry);
}
