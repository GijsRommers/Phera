//! Worker threads: one current-thread runtime, one `LocalSet` and one
//! listening socket each.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::Arc;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, mpsc, watch};

use crate::config::{ConnectionLimits, ServerConfig};
use crate::connection::{ConnectionParams, WebSocketWorkerContext, handle_connection};
use crate::handler::HandlerFactory;
use crate::websocket::WebSocketHub;

const BACKLOG: i32 = 1024;
const ACCEPT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// Bind a listening socket with `SO_REUSEADDR` and `SO_REUSEPORT` so every
/// worker can bind the same address and the kernel spreads connections.
#[cfg(unix)]
pub(super) fn bind_listener(addr: SocketAddr) -> io::Result<StdListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    // SO_REUSEPORT lets several sockets share one port, which is what the
    // per-worker sockets on an explicit port need. On an ephemeral port (0)
    // it would only invite two sockets landing on the same port, so skip it.
    if addr.port() != 0 {
        socket.set_reuse_port(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(BACKLOG)?;
    Ok(StdListener::from(socket))
}

/// Without `SO_REUSEPORT`, later workers reuse the first listener instead.
#[cfg(not(unix))]
pub(super) fn bind_listener(addr: SocketAddr) -> io::Result<StdListener> {
    StdListener::bind(addr)
}

/// A listener for worker 2..n. `requested` is the address the caller asked
/// for.
///
/// On Linux with an explicit port, each worker gets a fresh socket on the same
/// port and the kernel load-balances new connections between them. Everywhere
/// else the workers share the first socket: macOS and the BSDs accept
/// `SO_REUSEPORT` but hand every connection to one socket, which would leave
/// all but one worker idle. An ephemeral port is shared too, so no second
/// bind can race another process for it.
pub(super) fn extra_listener(
    first: &StdListener,
    requested: SocketAddr,
) -> io::Result<StdListener> {
    if cfg!(target_os = "linux") && requested.port() != 0 {
        return bind_listener(first.local_addr()?);
    }
    first.try_clone()
}

/// Run one worker until shutdown is requested, then drain its connections.
pub(super) fn run(
    index: usize,
    listener: StdListener,
    factory: Arc<dyn HandlerFactory>,
    ws_hub: Arc<WebSocketHub>,
    config: ServerConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("phera-server: worker {index} failed to start: {error}");
            return;
        }
    };
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(async move {
        // Built here, inside this worker's runtime, so its non-Send state
        // (Rc fields) never leaves this thread.
        let handler = factory.create(index);
        handler.start().await;
        let listener = {
            // Tokio refuses to register a blocking socket.
            let listener = listener;
            if let Err(error) = listener.set_nonblocking(true) {
                eprintln!("phera-server: worker {index} could not ready its socket: {error}");
                return;
            }
            match TcpListener::from_std(listener) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!(
                        "phera-server: worker {index} could not register its socket: {error}"
                    );
                    return;
                }
            }
        };
        let local_addr = listener
            .local_addr()
            .expect("listening socket has an address");
        let limits = ConnectionLimits::from(&config);
        let free_slots = Arc::new(Semaphore::new(config.max_connections_per_worker.max(1)));
        let ws_slots = Arc::new(Semaphore::new(
            config.websocket.max_connections_per_worker.max(1),
        ));
        let (http_done_tx, mut http_done_rx) = mpsc::channel::<()>(1);
        let (ws_done_tx, mut ws_done_rx) = mpsc::channel::<()>(1);
        let ws_config = config.websocket.clone();
        loop {
            // A slot first, then a connection: beyond the limit, clients
            // wait in the operating system's queue instead of being
            // accepted and costing memory here.
            let slot = tokio::select! {
                _ = wait_for_shutdown(&mut shutdown) => break,
                slot = free_slots.clone().acquire_owned() => slot,
            };
            let Ok(slot) = slot else { break };
            tokio::select! {
                _ = wait_for_shutdown(&mut shutdown) => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let handler = handler.clone();
                        let shutdown = shutdown.clone();
                        let http_done = http_done_tx.clone();
                        let ws_ctx = WebSocketWorkerContext {
                            hub: ws_hub.clone(),
                            config: ws_config.clone(),
                            slots: ws_slots.clone(),
                            done: ws_done_tx.clone(),
                        };
                        tokio::task::spawn_local(async move {
                            let _http_done = http_done;
                            handle_connection(ConnectionParams {
                                stream,
                                peer,
                                local_addr,
                                handler,
                                shutdown,
                                limits,
                                tcp_slot: slot,
                                ws: ws_ctx,
                            })
                            .await;
                        });
                    }
                    // A client that hung up before we accepted it is normal.
                    Err(error) if error.kind() == io::ErrorKind::ConnectionAborted => continue,
                    // Running out of file descriptors and similar errors pass;
                    // a worker that stopped accepting would never come back.
                    Err(error) => {
                        eprintln!("phera-server: worker {index} accept failed: {error}");
                        tokio::time::sleep(ACCEPT_RETRY_DELAY).await;
                    }
                },
            }
        }
        drop(listener);
        drop(http_done_tx);
        drop(ws_done_tx);
        // Drain HTTP connections up to `shutdown_timeout` while WebSockets send
        // `1001` and run `onClose` within `websocket.shutdown_grace`.
        let http_drain = tokio::time::timeout(config.shutdown_timeout, http_done_rx.recv());
        let ws_drain = tokio::time::timeout(config.websocket.shutdown_grace, ws_done_rx.recv());
        let _ = tokio::join!(http_drain, ws_drain);
    }));
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.changed().await;
}
