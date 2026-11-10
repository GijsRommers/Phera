//! A tiny server that proves non-`Send` handler state works: each worker
//! keeps its own `Rc<Cell<u64>>` request counter and reports it.
//!
//! Run with `cargo run -p phera-server --example hello` and try
//! `curl http://127.0.0.1:8080/x`.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use phera_server::{Handler, HandlerFactory, Request, Response, Server, ServerConfig};

struct Hello {
    worker_index: usize,
    requests: Rc<Cell<u64>>,
}

impl Handler for Hello {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            self.requests.set(self.requests.get() + 1);
            let count = self.requests.get();
            let query = if request.query.is_empty() {
                String::new()
            } else {
                format!("?{}", request.query)
            };
            let body = format!(
                "{} {}{} worker={} request #{} on this worker\n",
                request.method, request.path, query, self.worker_index, count
            );
            Response::text(200, &body)
        })
    }
}

struct HelloFactory;

impl HandlerFactory for HelloFactory {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Hello {
            worker_index,
            requests: Rc::new(Cell::new(0)),
        })
    }
}

fn main() {
    let server = Server::bind(ServerConfig::default(), HelloFactory).expect("failed to bind");
    eprintln!("listening on http://{}", server.local_addr());
    if let Err(error) = server.run() {
        eprintln!("phera-server: {error}");
        std::process::exit(1);
    }
}
