//! The server's configuration.

use std::net::SocketAddr;
use std::time::Duration;

/// The configuration of a server ([`ServerConfig`]).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub addr: SocketAddr,
    pub workers: usize,
    pub max_body_bytes: usize,
    pub header_read_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            workers: std::thread::available_parallelism().map_or(1, |n| n.get()),
            // PHP's post_max_size default, in bytes.
            max_body_bytes: 8 * 1024 * 1024,
            header_read_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(30),
        }
    }
}
