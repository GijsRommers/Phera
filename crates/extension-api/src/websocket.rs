//! Host contract between PHP's `Phera\WebSocket` API and the HTTP/WebSocket server.
//!
//! `crates/server` owns the sockets, rooms, and per-connection context bytes via
//! [`WebSocketSink`]. The VM holds `Option<Arc<dyn WebSocketSink>>` without
//! depending on `phera-server` or networking crates.

/// Controls and state exposed by the WebSocket hub to PHP scripts and event forks.
pub trait WebSocketSink: Send + Sync {
    /// Queues a WebSocket frame to a connected client. Returns `false` if unknown or closed.
    fn send(&self, connection_id: &str, data: &[u8], binary: bool) -> bool;

    /// Queues a frame to every connection in `room`, optionally skipping one connection.
    fn broadcast(
        &self,
        room: &str,
        data: &[u8],
        binary: bool,
        exclude_connection_id: Option<&str>,
    ) -> usize;

    /// Adds a live connection to `room`.
    fn join(&self, connection_id: &str, room: &str) -> bool;

    /// Removes a live connection from `room`.
    fn leave(&self, connection_id: &str, room: &str) -> bool;

    /// Returns the sorted list of rooms `connection_id` belongs to.
    fn rooms(&self, connection_id: &str) -> Vec<String>;

    /// Returns the number of live connections in `room`.
    fn room_count(&self, room: &str) -> usize;

    /// Returns the connection IDs currently in `room`.
    fn connections(&self, room: &str) -> Vec<String>;

    /// Initiates a clean WebSocket close for `connection_id`.
    fn close(&self, connection_id: &str, code: u16, reason: &str) -> bool;

    /// Returns the encoded context bytes for `connection_id`, if connected.
    fn context(&self, connection_id: &str) -> Option<Vec<u8>>;

    /// Replaces the encoded context bytes for `connection_id`.
    fn set_context(&self, connection_id: &str, context: Vec<u8>) -> bool;
}

/// Upgrade parameters recorded when PHP calls `Phera\WebSocket\accept()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketAccept {
    pub handler: String,
    pub context: Vec<u8>,
    pub rooms: Vec<String>,
    pub subprotocol: Option<String>,
}

/// One WebSocket lifecycle event delivered into an isolated PHP run.
#[derive(Debug, Clone)]
pub enum WebSocketEvent {
    Open,
    Message { data: Vec<u8>, binary: bool },
    Close { code: u16, reason: String },
}

/// Outcome of running one WebSocket lifecycle event in PHP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WebSocketEventOutcome {
    pub close_1011: bool,
}
