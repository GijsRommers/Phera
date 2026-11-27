//! The Phera HTTP server: HTTP/1.1 and HTTP/2 on Tokio, thread per core.
//!
//! One worker thread per core. Each worker runs its own current-thread
//! Tokio runtime with a `LocalSet`. With a single worker, it accepts directly
//! from the listening socket with no extra hop. With multiple workers, a
//! single listening socket accepts connections and hands each one to the
//! worker with the fewest active connections (round-robin on ties), so
//! keep-alive pools spread evenly across workers on every platform. Each
//! connection is handled with `tokio::task::spawn_local` on its worker's
//! runtime; tasks never cross threads, so handlers are free to hold `Rc` and
//! `RefCell` state.
//!
//! The PHP runtime plugs in through [`Handler`] and [`HandlerFactory`]:
//! the factory is `Send + Sync` and is called once per worker thread,
//! inside that worker's runtime, and returns the worker's handler.
//! The handler itself is usually non-`Send`, which is why its future is not.

#![forbid(unsafe_code)]

pub mod config;
mod connection;
mod file_body;
pub mod handler;
mod receive_timeout;
pub mod request;
pub mod response;
mod response_stream;
mod response_task;
mod server;
mod shutdown;
pub mod websocket;
mod worker;

pub use config::ServerConfig;
pub use handler::{Handler, HandlerFactory};
pub use request::{HttpVersion, Request};
pub use response::{Body, FileSlice, Response};
pub use response_stream::{ResponseWriter, StreamBody, StreamWriter};
pub use server::{Server, ServerHandle};
pub use websocket::{
    WebSocketConfig, WebSocketEvent, WebSocketEventKind, WebSocketHub, WebSocketOutcome,
    WebSocketUpgrade,
};
