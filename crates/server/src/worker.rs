//! Worker threads: one current-thread runtime, one `LocalSet` and one
//! listening socket each.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::rc::Rc;
use std::sync::Arc;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};

use crate::config::{ConnectionLimits, ServerConfig};
use crate::connection::handle_connection;
use crate::handler::HandlerFactory;
use crate::response_stream::ResponseSlot;
use crate::response_task::{ConnectionWork, DetachedRequests};

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
        let (stop_connections, _) = watch::channel(());
        let detached = Rc::new(DetachedRequests::new());
        loop {
            // A slot first, then a connection: beyond the limit, clients
            // wait in the operating system's queue instead of being
            // accepted and costing memory here.
            let slot = tokio::select! {
                _ = shutdown.changed() => break,
                slot = free_slots.clone().acquire_owned() => slot,
            };
            let Ok(slot) = slot else { break };
            tokio::select! {
                _ = shutdown.changed() => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let handler = handler.clone();
                        let watcher = stop_connections.subscribe();
                        let work = Rc::new(ConnectionWork {
                            _slot: slot,
                            detached: detached.clone(),
                            response: ResponseSlot::new(),
                        });
                        tokio::task::spawn_local(async move {
                            handle_connection(stream, peer, local_addr, handler, watcher, limits, work)
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
        // Finish in-flight requests and close idle keep-alive connections;
        // give up if something (a stuck upload) refuses to leave in time.
        let _ = tokio::time::timeout(config.shutdown_timeout, async {
            let _ = stop_connections.send(());
            stop_connections.closed().await;
            detached.drain().await;
        })
        .await;
        detached.stop();
    }));
}
