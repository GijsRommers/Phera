//! gRPC and Protocol Buffers for PHP, in safe Rust.
//!
//! ```php
//! use Phera\Grpc\{Server, Client, Context, Status, StatusException};
//! use Phera\Protobuf\Schema;
//!
//! $schema = Schema::define([
//!     'echo.Req'  => [1 => ['name' => 'message', 'type' => 'string']],
//!     'echo.Resp' => [1 => ['name' => 'message', 'type' => 'string']],
//! ]);
//!
//! // server.php
//! (new Server('127.0.0.1:50051', $schema))
//!     ->unary('/echo.Echo/Say', fn (array $req, Context $ctx) => ['message' => $req['message']],
//!             'echo.Req', 'echo.Resp')
//!     ->listen();
//!
//! // client.php
//! $client = new Client('127.0.0.1:50051', $schema);
//! $client->unary('/echo.Echo/Say', ['message' => 'hi'], 'echo.Req', 'echo.Resp');
//! ```
//!
//! The server calls PHP handlers directly on the thread that runs the
//! script: HTTP/2 connections are served by tasks on that same thread, and
//! each call reaches PHP through a queue in memory, never through another
//! process. The crate's README describes the API and the design.

mod client;
mod context;
mod framing;
mod headers;
pub mod protobuf;
mod server;
mod status;
mod tls;
mod transport;

use phera_extension_api::{Extension, Registry};

pub struct Grpc;

/// PHP declarations of every class here, checked against the registration.
pub const STUBS: &str = include_str!("../stubs.php");

impl Extension for Grpc {
    fn name(&self) -> &'static str {
        "phera-grpc"
    }

    /// Not PECL's `grpc` module: `extension_loaded('grpc')` stays false, so
    /// libraries that look for `Grpc\Channel` are not misled.
    fn php_module(&self) -> Option<&'static str> {
        None
    }

    fn register(&self, registry: &mut Registry) {
        protobuf::php::register(registry);
        status::register(registry);
        context::register(registry);
        server::register(registry);
        client::register(registry);
    }
}
