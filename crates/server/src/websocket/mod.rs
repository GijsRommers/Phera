//! Server-side WebSocket connections, handshake, hub, and framing.

pub mod handshake;
pub mod hub;
pub mod stream;

use std::time::Duration;

pub use hub::WebSocketHub;

/// Configuration and limits for WebSocket connections accepted by [`Server`].
///
/// [`Server`]: crate::Server
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Maximum open WebSocket connections held by one worker thread.
    pub max_connections_per_worker: usize,
    /// Maximum reassembled incoming message and frame payload size in bytes.
    pub max_message_bytes: usize,
    /// Close connections silent for this long (including Pong frames). Zero
    /// disables idle disconnection.
    pub idle_timeout: Duration,
    /// How often the server sends a keepalive Ping frame when non-zero.
    pub ping_interval: Duration,
    /// Maximum outbound or inbound queued messages per connection before
    /// disconnecting slow consumers or pausing reads.
    pub send_queue_len: usize,
    /// Maximum outbound queued payload bytes per connection before
    /// disconnecting slow consumers.
    pub send_queue_bytes: usize,
    /// Write deadline per outbound frame or close flush.
    pub write_timeout: Duration,
    /// Grace window on shutdown to send `1001` close frames and run `onClose`.
    pub shutdown_grace: Duration,
    /// Allowed browser `Origin` entries (`"*"` allows any origin). When
    /// `None`, browser `Origin` headers must match the effective request origin.
    pub allowed_origins: Option<Vec<String>>,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            max_connections_per_worker: 5_000,
            max_message_bytes: 1024 * 1024,
            idle_timeout: Duration::from_secs(90),
            ping_interval: Duration::from_secs(30),
            send_queue_len: 64,
            send_queue_bytes: 1024 * 1024,
            write_timeout: Duration::from_secs(15),
            shutdown_grace: Duration::from_secs(5),
            allowed_origins: None,
        }
    }
}

/// Upgrade details returned by a handler to accept a WebSocket connection.
#[derive(Debug, Clone)]
pub struct WebSocketUpgrade {
    /// Route or entry script identifier passed back in [`WebSocketEvent`].
    pub entry: String,
    /// Name of the handler class in the application.
    pub handler_class: String,
    /// Optional negotiated `Sec-WebSocket-Protocol` subprotocol.
    pub subprotocol: Option<String>,
    /// Opaque serialized context bag owned by the connection.
    pub context: Vec<u8>,
    /// Initial rooms subscribed right after the upgrade succeeds.
    pub rooms: Vec<String>,
}

/// One lifecycle event delivered in arrival order for a single connection.
#[derive(Debug, Clone)]
pub enum WebSocketEventKind {
    Open,
    Message { data: Vec<u8>, binary: bool },
    Close { code: u16, reason: String },
}

/// One WebSocket lifecycle event passed to [`crate::Handler::handle_websocket`].
#[derive(Debug, Clone)]
pub struct WebSocketEvent {
    pub connection_id: String,
    pub entry: String,
    pub handler_class: String,
    pub kind: WebSocketEventKind,
}

/// Result of running one WebSocket event handler.
#[derive(Debug, Clone, Default)]
pub struct WebSocketOutcome {
    /// Close the socket with code `1011` (`Internal Error`) after logging/error
    /// reporting when the handler failed unexpectedly.
    pub close_1011: bool,
}
