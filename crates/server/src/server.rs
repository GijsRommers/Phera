//! The server front: binding, worker threads and shutdown.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread::JoinHandle;

use tokio::sync::watch;

use crate::config::ServerConfig;
use crate::handler::HandlerFactory;
use crate::shutdown::spawn_signal_watcher;
use crate::websocket::WebSocketHub;
use crate::worker;

/// A server ready to run.
///
/// Bind with [`Server::bind`], then either [`run`](Server::run) on the
/// current thread (it blocks until SIGINT or SIGTERM) or
/// [`spawn`](Server::spawn) it on a background thread.
pub struct Server {
    config: ServerConfig,
    factory: Arc<dyn HandlerFactory>,
    ws_hub: Arc<WebSocketHub>,
    listeners: Vec<StdListener>,
    local_addr: SocketAddr,
    shutdown: watch::Sender<bool>,
}

impl Server {
    /// Bind one listening socket per worker and prepare the server.
    ///
    /// When the port in `config.addr` is 0 the kernel picks one for the first
    /// socket and every other worker binds to that same port.
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
        let workers = config.workers.max(1);
        let first = worker::bind_listener(config.addr)?;
        let local_addr = first.local_addr()?;
        let mut listeners = Vec::with_capacity(workers);
        listeners.push(first);
        for _ in 1..workers {
            listeners.push(worker::extra_listener(&listeners[0], config.addr)?);
        }
        let (shutdown, _) = watch::channel(false);
        Ok(Server {
            config,
            factory: Arc::new(factory),
            ws_hub,
            listeners,
            local_addr,
            shutdown,
        })
    }

    /// Process-wide WebSocket connection and room hub.
    pub fn websocket_hub(&self) -> Arc<WebSocketHub> {
        self.ws_hub.clone()
    }

    /// The address the first listening socket is bound to.
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

/// Start the workers, wait for a shutdown signal, and join everything.
fn run_supervisor(mut server: Server, with_signals: bool) -> io::Result<()> {
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let mut workers = Vec::new();
    for (index, listener) in server.listeners.drain(..).enumerate() {
        let factory = Arc::clone(&server.factory);
        let ws_hub = Arc::clone(&server.ws_hub);
        let config = server.config.clone();
        let shutdown = server.shutdown.subscribe();
        let done_tx = done_tx.clone();
        let workers_count = workers.len();
        let worker = std::thread::Builder::new()
            .name(format!("phera-worker-{index}"))
            .spawn(move || {
                worker::run(index, listener, factory, ws_hub, config, shutdown);
                let _ = done_tx.send(());
            })
            .unwrap_or_else(|error| panic!("failed to spawn worker {workers_count}: {error}"));
        workers.push(worker);
    }
    drop(done_tx);

    // Never joined: it only wakes on a signal, and the workers can also stop
    // for other reasons. It dies with the process.
    if with_signals {
        spawn_signal_watcher(server.shutdown.clone());
    }

    for _ in 0..workers.len() {
        let _ = done_rx.recv();
    }
    let mut result = Ok(());
    for worker in workers {
        if worker.join().is_err() {
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
        let _ = self.shutdown.send(true);
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
