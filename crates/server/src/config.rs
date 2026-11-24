//! The server's configuration, and the limits that keep one client from
//! taking a worker away from the others.

use std::net::SocketAddr;
use std::time::Duration;

use crate::websocket::WebSocketConfig;

/// The configuration of a server ([`ServerConfig`]).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub addr: SocketAddr,
    pub workers: usize,
    /// The largest request body, in bytes. A larger one is answered 413.
    pub max_body_bytes: usize,
    /// How long a connection may stay silent: before its first request,
    /// between two requests, and while the headers of a request arrive.
    /// After that the connection is closed without an answer.
    pub idle_timeout: Duration,
    /// The longest pause between two pieces of a request body. A client
    /// that stops in the middle of an upload is answered 408.
    pub body_idle_timeout: Duration,
    /// Base time for the complete request head and body. Body bytes received
    /// extend the original deadline in proportion to the bytes accepted.
    pub request_read_timeout: Duration,
    /// The longest time a handler may take to answer a complete request,
    /// waiting included. After that the handler is stopped and the client
    /// is answered 504. `None` means no limit.
    pub request_timeout: Option<Duration>,
    /// How many connections one worker serves at a time. Further ones wait
    /// in the operating system's queue until a connection closes.
    pub max_connections_per_worker: usize,
    pub shutdown_timeout: Duration,
    /// WebSocket limits, timeouts, and Origin allowlist.
    pub websocket: WebSocketConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            workers: std::thread::available_parallelism().map_or(1, |n| n.get()),
            // PHP's post_max_size default, in bytes.
            max_body_bytes: 8 * 1024 * 1024,
            // A proxy keeps its connections to Phera open and reuses them.
            // If Phera closed an idle one first, the proxy's next request
            // on it would fail. Proxies commonly drop idle connections
            // after 60 seconds, so Phera waits longer than that.
            idle_timeout: Duration::from_secs(75),
            body_idle_timeout: Duration::from_secs(30),
            request_read_timeout: Duration::from_secs(60),
            request_timeout: Some(Duration::from_secs(60)),
            max_connections_per_worker: 10_000,
            shutdown_timeout: Duration::from_secs(30),
            websocket: WebSocketConfig::default(),
        }
    }
}

/// The limits that apply to each connection, copied out of the
/// configuration once per worker.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConnectionLimits {
    pub(crate) max_body_bytes: usize,
    pub(crate) idle_timeout: Duration,
    pub(crate) body_idle_timeout: Duration,
    pub(crate) request_read_timeout: Duration,
    pub(crate) request_timeout: Option<Duration>,
}

impl From<&ServerConfig> for ConnectionLimits {
    fn from(config: &ServerConfig) -> Self {
        Self {
            max_body_bytes: config.max_body_bytes,
            idle_timeout: config.idle_timeout,
            body_idle_timeout: config.body_idle_timeout,
            request_read_timeout: config.request_read_timeout,
            request_timeout: config.request_timeout,
        }
    }
}
