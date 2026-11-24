//! Balanced connection distribution across multiple workers.

mod common;

use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response, ServerConfig};

struct WorkerId {
    worker_index: usize,
}

impl Handler for WorkerId {
    fn handle(&self, _request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        let body = self.worker_index.to_string();
        Box::pin(async move { Response::text(200, &body) })
    }
}

struct WorkerIdFactory;

impl HandlerFactory for WorkerIdFactory {
    fn create(&self, worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(WorkerId { worker_index })
    }
}

fn parse_worker_id(response: &common::Response) -> usize {
    assert_eq!(response.status, 200);
    std::str::from_utf8(&response.body)
        .expect("worker index is utf8")
        .parse()
        .expect("worker index is an integer")
}

#[test]
fn keepalive_connections_split_evenly_across_two_workers() {
    let (server, addr) = common::start(WorkerIdFactory, 2, 64 * 1024);

    // Open all 64 keep-alive connections before closing any, matching a
    // benchmark connection pool (c=64) across 2 workers.
    const CONNECTIONS: usize = 64;
    let mut clients: Vec<Client> = (0..CONNECTIONS).map(|_| Client::connect(addr)).collect();
    let mut per_worker = [0usize; 2];

    for client in &mut clients {
        let first = client.request("GET /first HTTP/1.1", &[], b"");
        assert_ne!(first.header("connection"), Some("close"));
        let worker = parse_worker_id(&first);

        // Second request on the same keep-alive connection stays on the same worker.
        let second = client.request("GET /second HTTP/1.1", &[], b"");
        assert_eq!(parse_worker_id(&second), worker);

        per_worker[worker] += 1;
    }

    assert_eq!(
        per_worker,
        [32, 32],
        "64 keep-alive connections should split 32/32 across 2 workers"
    );

    drop(clients);
    server.shutdown();
}

#[test]
fn keepalive_connections_split_evenly_across_four_workers() {
    let (server, addr) = common::start(WorkerIdFactory, 4, 64 * 1024);

    const CONNECTIONS: usize = 64;
    let mut clients: Vec<Client> = (0..CONNECTIONS).map(|_| Client::connect(addr)).collect();
    let mut per_worker = [0usize; 4];

    for client in &mut clients {
        let response = client.request("GET / HTTP/1.1", &[], b"");
        let worker = parse_worker_id(&response);
        per_worker[worker] += 1;
    }

    assert_eq!(
        per_worker,
        [16, 16, 16, 16],
        "64 keep-alive connections should split 16/16/16/16 across 4 workers"
    );

    drop(clients);
    server.shutdown();
}

#[test]
fn keepalive_connections_rebalance_after_disconnects() {
    let (server, addr) = common::start(WorkerIdFactory, 2, 64 * 1024);

    let mut worker0_clients = Vec::new();
    let mut worker1_clients = Vec::new();
    for _ in 0..32 {
        let mut client = Client::connect(addr);
        let worker = parse_worker_id(&client.request("GET / HTTP/1.1", &[], b""));
        if worker == 0 {
            worker0_clients.push(client);
        } else {
            worker1_clients.push(client);
        }
    }
    assert_eq!(worker0_clients.len(), 16);
    assert_eq!(worker1_clients.len(), 16);

    // Close 8 connections on worker 0, leaving active counts at 8 vs 16.
    worker0_clients.truncate(8);
    // Give worker 0 time to read EOF and release the 8 connection slots.
    std::thread::sleep(Duration::from_millis(150));

    // The next 8 connections should all route to the less-loaded worker 0.
    let mut new_clients = Vec::new();
    let mut new_per_worker = [0usize; 2];
    for _ in 0..8 {
        let mut client = Client::connect(addr);
        let worker = parse_worker_id(&client.request("GET / HTTP/1.1", &[], b""));
        new_per_worker[worker] += 1;
        new_clients.push(client);
    }

    assert_eq!(
        new_per_worker,
        [8, 0],
        "new connections should refill the lighter worker first"
    );

    drop(new_clients);
    drop(worker0_clients);
    drop(worker1_clients);
    server.shutdown();
}

#[test]
fn multi_worker_connection_limit_waits_for_free_slot() {
    let config = ServerConfig {
        workers: 2,
        max_connections_per_worker: 2,
        ..common::test_config()
    };
    let (server, addr) = common::start_with(WorkerIdFactory, config);

    // Fill all 4 slots (2 per worker).
    let mut clients: Vec<Client> = (0..4).map(|_| Client::connect(addr)).collect();
    let mut per_worker = [0usize; 2];
    for client in &mut clients {
        let worker = parse_worker_id(&client.request("GET / HTTP/1.1", &[], b""));
        per_worker[worker] += 1;
    }
    assert_eq!(per_worker, [2, 2]);

    // A 5th client waits in the OS queue while all worker slots are occupied.
    let mut fifth = TcpStream::connect(addr).unwrap();
    fifth
        .write_all(b"GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
        .unwrap();
    fifth
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut answer = [0u8; 512];
    let waiting = fifth.read(&mut answer);
    assert!(
        waiting.is_err(),
        "the 5th client should wait while all 4 slots are held: {waiting:?}"
    );

    // Releasing one slot allows the 5th client to be accepted and served.
    clients.pop();
    fifth
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let read = fifth
        .read(&mut answer)
        .expect("5th client should be served once a slot opens");
    assert!(answer[..read].starts_with(b"HTTP/1.1 200"));

    drop(clients);
    server.shutdown();
}
