//! The server front: binding, worker threads and shutdown.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::thread::JoinHandle;

use tokio::sync::{Notify, mpsc, oneshot, watch};

use crate::config::ServerConfig;
use crate::handler::HandlerFactory;
use crate::shutdown::spawn_signal_watcher;
use crate::websocket::WebSocketHub;
use crate::worker::{self, WorkerSlot};

/// A server ready to run.
///
/// Bind with [`Server::bind`], then either [`run`](Server::run) on the
/// current thread (it blocks until SIGINT or SIGTERM) or
/// [`spawn`](Server::spawn) it on a background thread.
pub struct Server {
    config: ServerConfig,
    factory: Arc<dyn HandlerFactory>,
    ws_hub: Arc<WebSocketHub>,
    listener: StdListener,
    local_addr: SocketAddr,
    shutdown: watch::Sender<bool>,
}

impl Server {
    /// Bind the listening socket and prepare the server.
    pub fn bind(config: ServerConfig, factory: impl HandlerFactory) -> io::Result<Server> {
        let ws_hub = Arc::new(WebSocketHub::new(
            config.websocket.send_queue_len,
            config.websocket.send_queue_bytes,
        ));
        Self::bind_with_hub(config, factory, ws_hub)
    }

    /// Bind using an explicit process-wide [`WebSocketHub`].
    pub fn bind_with_hub(
        config: ServerConfig,
        factory: impl HandlerFactory,
        ws_hub: Arc<WebSocketHub>,
    ) -> io::Result<Server> {
        let listener = worker::bind_listener(config.addr)?;
        let local_addr = listener.local_addr()?;
        let (shutdown, _) = watch::channel(false);
        Ok(Server {
            config,
            factory: Arc::new(factory),
            ws_hub,
            listener,
            local_addr,
            shutdown,
        })
    }

    /// Process-wide WebSocket connection and room hub.
    pub fn websocket_hub(&self) -> Arc<WebSocketHub> {
        self.ws_hub.clone()
    }

    /// The address the listening socket is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Run on the current thread until SIGINT or SIGTERM, then drain
    /// in-flight requests up to `shutdown_timeout`. Blocks until the workers
    /// have stopped.
    pub fn run(self) -> io::Result<()> {
        run_supervisor(self, true)
    }

    /// Run on a background thread and return a handle to stop it later.
    pub fn spawn(self) -> ServerHandle {
        let local_addr = self.local_addr;
        let shutdown = self.shutdown.clone();
        let supervisor = std::thread::Builder::new()
            .name("phera-server".to_string())
            .spawn(move || {
                let _ = run_supervisor(self, false);
            })
            .expect("failed to spawn the server thread");
        ServerHandle {
            local_addr,
            shutdown,
            supervisor: Mutex::new(Some(supervisor)),
        }
    }
}

/// Start the workers, accept/dispatch connections, wait for shutdown, and join
/// everything.
fn run_supervisor(server: Server, with_signals: bool) -> io::Result<()> {
    let Server {
        config,
        factory,
        ws_hub,
        listener,
        local_addr,
        shutdown,
    } = server;

    // Never joined: it only wakes on a signal, and the workers can also stop
    // for other reasons. It dies with the process.
    if with_signals {
        spawn_signal_watcher(shutdown.clone());
    }

    let worker_count = config.workers.max(1);
    if worker_count == 1 {
        let shutdown_rx = shutdown.subscribe();
        let worker = std::thread::Builder::new()
            .name("phera-worker-0".to_string())
            .spawn(move || {
                worker::run_direct(0, listener, factory, ws_hub, config, shutdown_rx);
            })
            .unwrap_or_else(|error| panic!("failed to spawn worker 0: {error}"));
        return join_workers(vec![worker], Ok(()));
    }

    let slot_freed = Arc::new(Notify::new());
    let max_per_worker = config.max_connections_per_worker.max(1);
    let mut workers = Vec::with_capacity(worker_count);
    let mut slots = Vec::with_capacity(worker_count);
    let mut ready_rxs = Vec::with_capacity(worker_count);

    for index in 0..worker_count {
        let (tx, rx) = mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let active = Arc::new(AtomicUsize::new(0));
        slots.push(WorkerSlot { tx, active });
        ready_rxs.push(ready_rx);

        let factory = Arc::clone(&factory);
        let ws_hub = Arc::clone(&ws_hub);
        let config = config.clone();
        let slot_freed = Arc::clone(&slot_freed);
        let shutdown_rx = shutdown.subscribe();
        let worker = std::thread::Builder::new()
            .name(format!("phera-worker-{index}"))
            .spawn(move || {
                worker::run_dispatched(
                    index,
                    local_addr,
                    rx,
                    ready_tx,
                    slot_freed,
                    factory,
                    ws_hub,
                    config,
                    shutdown_rx,
                );
            })
            .unwrap_or_else(|error| panic!("failed to spawn worker {index}: {error}"));
        workers.push(worker);
    }

    let accept_result = worker::run_acceptor(
        listener,
        slots,
        ready_rxs,
        slot_freed,
        max_per_worker,
        shutdown.subscribe(),
    );

    join_workers(workers, accept_result)
}

fn join_workers(workers: Vec<JoinHandle<()>>, accept_result: io::Result<()>) -> io::Result<()> {
    let mut result = accept_result;
    for worker in workers {
        if worker.join().is_err() && result.is_ok() {
            result = Err(io::Error::other("a phera-server worker thread panicked"));
        }
    }
    result
}

/// A handle to a server started with [`Server::spawn`].
pub struct ServerHandle {
    local_addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

impl ServerHandle {
    /// The address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop accepting, let in-flight requests finish up to the shutdown
    /// timeout, and join the worker threads. Blocks until they are gone.
    pub fn shutdown(&self) {
        self.shutdown.send_replace(true);
        if let Some(supervisor) = self.supervisor.lock().unwrap().take() {
            let _ = supervisor.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}
