//! The port-0 listener binds must never collide: every test server binds
//! `127.0.0.1:0`, and with `SO_REUSEPORT` two sockets can land on the same
//! ephemeral port. This guards that binding many servers in one process
//! always yields distinct ports.

use std::collections::HashSet;
use std::pin::Pin;
use std::rc::Rc;

use phera_server::{Handler, HandlerFactory, Request, Response, Server, ServerConfig};

/// A factory whose handlers do nothing; only the bind is under test.
struct Idle;

impl HandlerFactory for Idle {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(IdleHandler { worker_index })
    }
}

struct IdleHandler {
    #[allow(dead_code)]
    worker_index: usize,
}

impl Handler for IdleHandler {
    fn handle(&self, _request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async { Response::new(200) })
    }
}

#[test]
fn repeated_port_zero_binds_never_collide() {
    let mut seen = HashSet::new();
    let mut servers = Vec::new();
    for _ in 0..64 {
        let config = ServerConfig {
            addr: "127.0.0.1:0".parse().unwrap(),
            workers: 2,
            ..Default::default()
        };
        let server = Server::bind(config, Idle).unwrap();
        // Keep the server alive until the next bind: a dropped socket is free
        // immediately, so uniqueness is only meaningful between live listeners.
        let port = server.local_addr().port();
        assert!(
            seen.insert(port),
            "two binds landed on the same port {port}"
        );
        servers.push(server);
    }
    assert_eq!(seen.len(), 64, "64 binds produced fewer than 64 ports");
    drop(servers);
}
