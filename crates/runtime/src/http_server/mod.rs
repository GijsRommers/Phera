//! `Phera\Http`: fast programmatic routing and request/response classes
//! plugged directly into `phera serve`.
//!
//! ```php
//! use Phera\Http\{Router, Request, Response};
//!
//! $router = new Router();
//! $router->onStart(function (int $worker) use (&$db) {
//!     $db = new PDO('sqlite:app.db');
//! });
//! $router
//!     ->get('/plaintext', fn() => 'Hello, World!')
//!     ->get('/users/{id}', fn(Request $r) => ['id' => (int) $r->param('id')]);
//!
//! return $router; // run with: phera serve app.php
//! ```
//!
//! Each `phera serve` worker boots the script once, freezes a snapshot of the
//! booted VM, and keeps a local pool of persistent VM instances (`idle: Vec`)
//! reused across requests. Fast sync routes stay at one warm VM per worker
//! with zero forks and zero channel hops; concurrent async I/O routes
//! automatically fork additional persistent instances up to the worker's
//! concurrency cap so requests multiplex M:N on the worker's `LocalSet`.

mod pool;
mod request;
mod response;
mod router;
mod router_obj;

use phera_extension_api::Registry;

pub use pool::{RouteBootReport, RouteDispatch, WorkerRouter};
pub use router_obj::PendingServe;

pub(crate) use response::{encode_json, received};
pub(crate) use router_obj::{ActiveRouterSlot, BootRouterSlot, PendingServeSlot};

pub(crate) const ROUTER_CLASS: &str = "Phera\\Http\\Router";
pub(crate) const REQUEST_CLASS: &str = "Phera\\Http\\Request";
pub(crate) const RESPONSE_CLASS: &str = "Phera\\Http\\Response";
pub(crate) const SERVE_FUNCTION: &str = "Phera\\Http\\serve";

/// Registers the `Phera\Http` classes and `Phera\Http\serve()`.
pub(crate) fn register(registry: &mut Registry) {
    registry.fork_state::<BootRouterSlot>(phera_extension_api::fresh_state);
    registry.fork_state::<PendingServeSlot>(phera_extension_api::fresh_state);
    registry.fork_state::<ActiveRouterSlot>(router_obj::fork_active_router_slot);
    request::register(registry);
    response::register(registry);
    router_obj::register(registry);
}
