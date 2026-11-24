//! Worker threads and connection distribution.
//!
//! Each worker runs its own current-thread Tokio runtime with a `LocalSet`.
//! With a single worker, [`run_direct`] accepts straight off the listening
//! socket with zero channel overhead. With multiple workers, [`run_acceptor`]
//! accepts from a single listening socket and dispatches each accepted socket
//! to the worker with the fewest active connections (round-robin on ties),
//! where [`run_dispatched`] registers it on that worker's own reactor.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener, TcpStream as StdStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)]
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};

use crate::config::{ConnectionLimits, ServerConfig};
use crate::connection::{ConnectionParams, WebSocketWorkerContext, handle_connection};
use crate::handler::HandlerFactory;
use crate::websocket::WebSocketHub;

const BACKLOG: i32 = 1024;
const ACCEPT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// Bind a single non-blocking listening socket with `SO_REUSEADDR`.
#[cfg(unix)]
pub(super) fn bind_listener(addr: SocketAddr) -> io::Result<StdListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(BACKLOG)?;
    Ok(StdListener::from(socket))
}

#[cfg(not(unix))]
pub(super) fn bind_listener(addr: SocketAddr) -> io::Result<StdListener> {
    let listener = StdListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Tracks one occupied TCP connection slot on a worker.
///
/// Held as an RAII guard for the lifetime of the TCP connection, including
/// after an HTTP/1.1 connection upgrades to a WebSocket.
#[allow(dead_code)]
pub(crate) enum ConnectionSlot {
    /// Single-worker direct accept mode: holds a local semaphore permit.
    Direct(OwnedSemaphorePermit),
    /// Multi-worker dispatched mode: decrements the worker's active counter
    /// and wakes the acceptor if it was waiting for capacity.
    Dispatched(DispatchedSlot),
}

pub(crate) struct DispatchedSlot {
    active: Arc<AtomicUsize>,
    slot_freed: Arc<Notify>,
}

impl Drop for DispatchedSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
        self.slot_freed.notify_one();
    }
}

/// An accepted TCP connection handed from the acceptor to a worker thread.
pub(super) struct Accepted {
    stream: StdStream,
    peer: SocketAddr,
    slot: ConnectionSlot,
}

/// Per-worker dispatch state owned by the acceptor.
pub(super) struct WorkerSlot {
    pub(super) tx: mpsc::UnboundedSender<Accepted>,
    pub(super) active: Arc<AtomicUsize>,
}

/// Ensures the acceptor wakes up if a worker thread exits unexpectedly while
/// the acceptor is waiting for capacity.
struct WorkerExitGuard {
    slot_freed: Arc<Notify>,
}

impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        self.slot_freed.notify_one();
    }
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|&stopped| stopped).await;
}

fn is_transient_accept_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::Interrupted
    )
}

/// Run a single worker that accepts directly on its own runtime (no channel hop).
pub(super) fn run_direct(
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
                                tcp_slot: ConnectionSlot::Direct(slot),
                                ws: ws_ctx,
                            })
                            .await;
                        });
                    }
                    // A client that hung up before we accepted it is normal.
                    Err(error) if is_transient_accept_error(&error) => continue,
                    // Running out of file descriptors and similar errors pass;
                    // a worker that stopped accepting would never come back.
                    Err(error) => {
                        eprintln!("phera-server: worker {index} accept failed: {error}");
                        tokio::select! {
                            _ = wait_for_shutdown(&mut shutdown) => break,
                            _ = tokio::time::sleep(ACCEPT_RETRY_DELAY) => {}
                        }
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

/// Run one worker in multi-worker mode, receiving accepted sockets over `rx`
/// and registering each on this worker's own Tokio reactor.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_dispatched(
    index: usize,
    local_addr: SocketAddr,
    rx: mpsc::UnboundedReceiver<Accepted>,
    ready_tx: oneshot::Sender<()>,
    slot_freed: Arc<Notify>,
    factory: Arc<dyn HandlerFactory>,
    ws_hub: Arc<WebSocketHub>,
    config: ServerConfig,
    shutdown: watch::Receiver<bool>,
) {
    // Declared before `rx` so that on any exit (including panic), `rx` drops
    // first (closing the channel) and then `_exit_guard` wakes the acceptor.
    let _exit_guard = WorkerExitGuard { slot_freed };
    let mut rx = rx;

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
        let _ = ready_tx.send(());

        let limits = ConnectionLimits::from(&config);
        let ws_slots = Arc::new(Semaphore::new(
            config.websocket.max_connections_per_worker.max(1),
        ));
        let (http_done_tx, mut http_done_rx) = mpsc::channel::<()>(1);
        let (ws_done_tx, mut ws_done_rx) = mpsc::channel::<()>(1);
        let ws_config = config.websocket.clone();

        while let Some(Accepted { stream, peer, slot }) = rx.recv().await {
            if let Err(error) = stream.set_nonblocking(true) {
                eprintln!("phera-server: worker {index} could not ready socket: {error}");
                continue;
            }
            let stream = match TcpStream::from_std(stream) {
                Ok(stream) => stream,
                Err(error) => {
                    eprintln!("phera-server: worker {index} could not register socket: {error}");
                    continue;
                }
            };
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

        drop(http_done_tx);
        drop(ws_done_tx);
        // Drain HTTP connections up to `shutdown_timeout` while WebSockets send
        // `1001` and run `onClose` within `websocket.shutdown_grace`.
        let http_drain = tokio::time::timeout(config.shutdown_timeout, http_done_rx.recv());
        let ws_drain = tokio::time::timeout(config.websocket.shutdown_grace, ws_done_rx.recv());
        let _ = tokio::join!(http_drain, ws_drain);
    }));
}

enum CapacityState {
    Available,
    Shutdown,
    AllWorkersClosed,
}

fn has_capacity(workers: &[WorkerSlot], max_per_worker: usize) -> bool {
    workers.iter().any(|worker| {
        !worker.tx.is_closed() && worker.active.load(Ordering::Acquire) < max_per_worker
    })
}

fn all_workers_closed(workers: &[WorkerSlot]) -> bool {
    workers.iter().all(|worker| worker.tx.is_closed())
}

/// Pick the alive worker with the fewest active connections below
/// `max_per_worker`, breaking ties round-robin starting from `*cursor`.
fn pick_worker(workers: &[WorkerSlot], max_per_worker: usize, cursor: &mut usize) -> Option<usize> {
    let count = workers.len();
    let mut best: Option<(usize, usize)> = None;
    for offset in 0..count {
        let index = (*cursor + offset) % count;
        let worker = &workers[index];
        if worker.tx.is_closed() {
            continue;
        }
        let active = worker.active.load(Ordering::Acquire);
        if active >= max_per_worker {
            continue;
        }
        match best {
            None => best = Some((index, active)),
            Some((_, best_active)) if active < best_active => {
                best = Some((index, active));
            }
            _ => {}
        }
        // Zero active connections is the global minimum; no need to scan further.
        if active == 0 {
            break;
        }
    }
    if let Some((index, _)) = best {
        *cursor = (index + 1) % count;
        Some(index)
    } else {
        None
    }
}

async fn wait_for_capacity(
    workers: &[WorkerSlot],
    max_per_worker: usize,
    slot_freed: &Notify,
    shutdown: &mut watch::Receiver<bool>,
) -> CapacityState {
    loop {
        if has_capacity(workers, max_per_worker) {
            return CapacityState::Available;
        }
        if all_workers_closed(workers) {
            return CapacityState::AllWorkersClosed;
        }
        let notified = slot_freed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        // Re-check after registering so a slot freed right before `enable()`
        // cannot be missed.
        if has_capacity(workers, max_per_worker) {
            return CapacityState::Available;
        }
        if all_workers_closed(workers) {
            return CapacityState::AllWorkersClosed;
        }
        tokio::select! {
            _ = wait_for_shutdown(shutdown) => return CapacityState::Shutdown,
            _ = notified => {}
        }
    }
}

async fn wait_for_workers_ready(
    ready_rxs: Vec<oneshot::Receiver<()>>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    for rx in ready_rxs {
        tokio::select! {
            _ = wait_for_shutdown(shutdown) => return false,
            _ = rx => {}
        }
    }
    true
}

async fn dispatch_connection(
    mut stream: StdStream,
    peer: SocketAddr,
    workers: &[WorkerSlot],
    max_per_worker: usize,
    cursor: &mut usize,
    slot_freed: &Arc<Notify>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    loop {
        if let Some(index) = pick_worker(workers, max_per_worker, cursor) {
            let worker = &workers[index];
            worker.active.fetch_add(1, Ordering::AcqRel);
            let slot = ConnectionSlot::Dispatched(DispatchedSlot {
                active: Arc::clone(&worker.active),
                slot_freed: Arc::clone(slot_freed),
            });
            match worker.tx.send(Accepted { stream, peer, slot }) {
                Ok(()) => return true,
                Err(mpsc::error::SendError(rejected)) => {
                    // Worker exited concurrently; `rejected.slot` drops here
                    // and decrements its count before we try another worker.
                    stream = rejected.stream;
                    continue;
                }
            }
        }
        match wait_for_capacity(workers, max_per_worker, slot_freed, shutdown).await {
            CapacityState::Available => continue,
            CapacityState::Shutdown | CapacityState::AllWorkersClosed => return false,
        }
    }
}

/// Accept one connection and detach it from the acceptor's reactor so the
/// receiving worker registers the socket on its own Tokio runtime.
async fn accept_std(listener: &TcpListener) -> io::Result<(StdStream, SocketAddr)> {
    let (stream, peer) = listener.accept().await?;
    Ok((stream.into_std()?, peer))
}

/// Accept connections from the single listening socket and dispatch them
/// evenly across `workers` until shutdown or all workers exit.
pub(super) fn run_acceptor(
    listener: StdListener,
    workers: Vec<WorkerSlot>,
    ready_rxs: Vec<oneshot::Receiver<()>>,
    slot_freed: Arc<Notify>,
    max_per_worker: usize,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        if !wait_for_workers_ready(ready_rxs, &mut shutdown).await {
            drop(listener);
            drop(workers);
            return Ok(());
        }

        listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(listener)?;

        let mut cursor = 0usize;
        loop {
            match wait_for_capacity(&workers, max_per_worker, &slot_freed, &mut shutdown).await {
                CapacityState::Available => {}
                CapacityState::Shutdown | CapacityState::AllWorkersClosed => break,
            }
            tokio::select! {
                _ = wait_for_shutdown(&mut shutdown) => break,
                accepted = accept_std(&listener) => match accepted {
                    Ok((stream, peer)) => {
                        if !dispatch_connection(
                            stream,
                            peer,
                            &workers,
                            max_per_worker,
                            &mut cursor,
                            &slot_freed,
                            &mut shutdown,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(error) if is_transient_accept_error(&error) => continue,
                    Err(error) => {
                        eprintln!("phera-server: accept failed: {error}");
                        tokio::select! {
                            _ = wait_for_shutdown(&mut shutdown) => break,
                            _ = tokio::time::sleep(ACCEPT_RETRY_DELAY) => {}
                        }
                    }
                },
            }
        }
        // Close the listening socket first, then drop all worker channels so
        // every worker exits its receive loop and drains in-flight requests.
        drop(listener);
        drop(workers);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_slots(counts: &[usize]) -> (Vec<WorkerSlot>, Vec<mpsc::UnboundedReceiver<Accepted>>) {
        let mut slots = Vec::with_capacity(counts.len());
        let mut rxs = Vec::with_capacity(counts.len());
        for &count in counts {
            let (tx, rx) = mpsc::unbounded_channel();
            slots.push(WorkerSlot {
                tx,
                active: Arc::new(AtomicUsize::new(count)),
            });
            rxs.push(rx);
        }
        (slots, rxs)
    }

    #[test]
    fn pick_worker_splits_burst_evenly_across_two_workers() {
        let (slots, _rxs) = make_slots(&[0, 0]);
        let mut cursor = 0;
        let mut assigned = [0usize; 2];
        for _ in 0..64 {
            let idx = pick_worker(&slots, 100, &mut cursor).expect("slot available");
            slots[idx].active.fetch_add(1, Ordering::AcqRel);
            assigned[idx] += 1;
        }
        assert_eq!(assigned, [32, 32]);
    }

    #[test]
    fn pick_worker_splits_burst_evenly_across_four_workers() {
        let (slots, _rxs) = make_slots(&[0, 0, 0, 0]);
        let mut cursor = 0;
        let mut assigned = [0usize; 4];
        for _ in 0..64 {
            let idx = pick_worker(&slots, 100, &mut cursor).expect("slot available");
            slots[idx].active.fetch_add(1, Ordering::AcqRel);
            assigned[idx] += 1;
        }
        assert_eq!(assigned, [16, 16, 16, 16]);
    }

    #[test]
    fn pick_worker_prefers_least_loaded_worker() {
        let (slots, _rxs) = make_slots(&[10, 4, 10]);
        let mut cursor = 0;
        for _ in 0..6 {
            let idx = pick_worker(&slots, 100, &mut cursor).expect("slot available");
            assert_eq!(idx, 1);
            slots[idx].active.fetch_add(1, Ordering::AcqRel);
        }
        assert_eq!(slots[1].active.load(Ordering::Acquire), 10);
    }

    #[test]
    fn pick_worker_respects_capacity_and_skips_closed_workers() {
        let (slots, mut rxs) = make_slots(&[2, 0, 1]);
        // Close worker 1; worker 0 is at capacity 2; only worker 2 is eligible.
        drop(rxs.remove(1));
        let mut cursor = 0;
        assert_eq!(pick_worker(&slots, 2, &mut cursor), Some(2));
        slots[2].active.fetch_add(1, Ordering::AcqRel);
        // Now worker 0 and 2 are full and worker 1 is closed.
        assert_eq!(pick_worker(&slots, 2, &mut cursor), None);
    }
}
