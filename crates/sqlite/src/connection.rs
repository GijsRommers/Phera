//! `exec('BEGIN; INSERT ...; COMMIT')` runs entirely on the database's owning thread.

use std::cell::Cell;
#[cfg(unix)]
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::{mpsc, oneshot};

use crate::data::{SQLITE_ERROR, SQLITE_LOCKED, SQLITE_MISUSE};
use crate::{Error, Statement};

/// Maximum idle SQLite worker threads kept parked for reuse. Each open
/// connection exclusively owns its worker thread until closed/dropped (preserving
/// full lock/transaction isolation without deadlocks), then returns the thread
/// with its live Tokio runtime and allocator heap to avoid per-request OS thread
/// creation and mimalloc segment abandonment under concurrent web load.
const MAX_IDLE_WORKERS: usize = 64;

/// How long an idle worker retains a clean, read-only file-backed SQLite
/// connection before closing it while staying parked in the worker pool.
#[cfg(unix)]
const IDLE_CACHE_TTL: Duration = Duration::from_secs(5);

type Operation = Box<dyn FnOnce(&mut Rc<Connection>) + Send>;
enum Request {
    Call {
        operation: Operation,
        mutates: bool,
    },
    Prepare {
        database: Database,
        sql: String,
        ready: oneshot::Sender<Result<Statement, Error>>,
    },
    Close(oneshot::Sender<()>),
    FinishCursor(tokio::task::JoinHandle<()>),
}

struct OpenTask {
    filename: String,
    flags: i32,
    timeout_ms: u64,
    ready: oneshot::Sender<Result<(), Error>>,
    requests: mpsc::UnboundedReceiver<Request>,
}

enum WorkerMessage {
    Open(OpenTask),
    #[cfg(unix)]
    Evict(std::sync::mpsc::SyncSender<()>),
}

struct IdleWorker {
    sender: std::sync::mpsc::SyncSender<WorkerMessage>,
    #[cfg(unix)]
    cached_key: Option<(String, i32, (u64, u64))>,
}

struct PoolState {
    workers: Vec<IdleWorker>,
    #[cfg(unix)]
    active_by_file: HashMap<(u64, u64), usize>,
}

#[cfg(unix)]
struct CachedConnection {
    filename: String,
    flags: i32,
    timeout_ms: u64,
    file_id: (u64, u64),
    connection: Rc<Connection>,
}

#[cfg(not(unix))]
struct CachedConnection;

#[cfg(unix)]
fn file_identity(path: &str) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

fn cacheable_path(filename: &str, flags: OpenFlags) -> bool {
    !filename.is_empty()
        && filename != ":memory:"
        && !filename.starts_with("file:")
        && !flags.contains(OpenFlags::SQLITE_OPEN_MEMORY)
}

fn idle_workers() -> &'static Mutex<PoolState> {
    static POOL: OnceLock<Mutex<PoolState>> = OnceLock::new();
    POOL.get_or_init(|| {
        Mutex::new(PoolState {
            workers: Vec::new(),
            #[cfg(unix)]
            active_by_file: HashMap::new(),
        })
    })
}

fn pop_idle_worker(filename: &str, flags: i32) -> Option<IdleWorker> {
    let mut pool = idle_workers()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    #[cfg(unix)]
    if let Some(pos) = pool.workers.iter().rposition(|worker| {
        worker
            .cached_key
            .as_ref()
            .is_some_and(|(f, fl, _)| *fl == flags && f == filename)
    }) {
        return Some(pool.workers.swap_remove(pos));
    }
    let _ = (filename, flags);
    pool.workers.pop()
}

#[cfg(unix)]
fn register_active_file(id: (u64, u64)) {
    let mut pool = idle_workers()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    *pool.active_by_file.entry(id).or_insert(0) += 1;
}

#[cfg(unix)]
enum ReleaseOutcome {
    /// Parked cleanly inside the pool while keeping our cached connection.
    ParkedCached(std::sync::mpsc::Receiver<WorkerMessage>),
    /// Pool was full; caller must drop its connection and exit the worker thread.
    PoolFull,
    /// Must drop our connection, evict any listed cached peers, then park clean.
    DropAndEvict(Vec<std::sync::mpsc::SyncSender<WorkerMessage>>),
}

#[cfg(unix)]
fn release_and_maybe_park(
    id: (u64, u64),
    can_cache: bool,
    filename: &str,
    flags: i32,
) -> ReleaseOutcome {
    let mut pool = idle_workers()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let remaining = match pool.active_by_file.get_mut(&id) {
        Some(count) if *count > 1 => {
            *count -= 1;
            *count
        }
        Some(_) => {
            pool.active_by_file.remove(&id);
            0
        }
        None => 0,
    };
    if remaining > 0 {
        if can_cache && file_identity(filename).is_some_and(|current| current == id) {
            if pool.workers.len() < MAX_IDLE_WORKERS {
                let (sender, receiver) = std::sync::mpsc::sync_channel::<WorkerMessage>(1);
                pool.workers.push(IdleWorker {
                    sender,
                    cached_key: Some((filename.to_owned(), flags, id)),
                });
                return ReleaseOutcome::ParkedCached(receiver);
            }
            return ReleaseOutcome::PoolFull;
        }
        return ReleaseOutcome::DropAndEvict(Vec::new());
    }
    let mut evict = Vec::new();
    let mut index = 0;
    while index < pool.workers.len() {
        let matches = pool.workers[index]
            .cached_key
            .as_ref()
            .is_some_and(|(_, _, cached_id)| *cached_id == id);
        if matches {
            evict.push(pool.workers.swap_remove(index).sender);
        } else {
            index += 1;
        }
    }
    ReleaseOutcome::DropAndEvict(evict)
}

#[cfg(unix)]
fn evict_workers(senders: Vec<std::sync::mpsc::SyncSender<WorkerMessage>>) {
    // Evict sequentially so the final WAL-attached connection closes alone
    // without contending on SQLite's non-blocking WAL-close exclusive lock.
    for sender in senders {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        if sender.send(WorkerMessage::Evict(tx)).is_ok() {
            let _ = rx.recv();
        }
    }
}

fn return_idle_worker(worker: IdleWorker) -> bool {
    let mut pool = idle_workers()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if pool.workers.len() < MAX_IDLE_WORKERS {
        pool.workers.push(worker);
        true
    } else {
        false
    }
}

enum Served {
    /// Connection was dropped; worker should park cleanly with no cached connection.
    Clean,
    /// Worker is already parked in the pool with a live cached connection and receiver.
    #[cfg(unix)]
    ParkedCached(CachedConnection, std::sync::mpsc::Receiver<WorkerMessage>),
    /// Pool is full; worker thread should exit.
    #[cfg(unix)]
    Exit,
}

fn run_worker(first: OpenTask) {
    let runtime = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = first.ready.send(Err(Error {
                code: SQLITE_ERROR,
                message: error.to_string(),
            }));
            return;
        }
    };
    let mut served = serve_connection(&runtime, first, None);
    loop {
        let (mut cached, receiver) = match served {
            Served::Clean => {
                let (sender, receiver) = std::sync::mpsc::sync_channel::<WorkerMessage>(1);
                let worker = IdleWorker {
                    sender,
                    #[cfg(unix)]
                    cached_key: None,
                };
                if !return_idle_worker(worker) {
                    return;
                }
                (None, receiver)
            }
            #[cfg(unix)]
            Served::ParkedCached(entry, receiver) => (Some(entry), receiver),
            #[cfg(unix)]
            Served::Exit => return,
        };
        let next = recv_next_task(&receiver, &mut cached);
        match next {
            Some(WorkerMessage::Open(task)) => {
                served = serve_connection(&runtime, task, cached);
            }
            #[cfg(unix)]
            Some(WorkerMessage::Evict(ack)) => {
                drop(cached);
                let _ = ack.send(());
                served = Served::Clean;
            }
            None => return,
        }
    }
}

#[cfg(unix)]
fn recv_next_task(
    receiver: &std::sync::mpsc::Receiver<WorkerMessage>,
    cached: &mut Option<CachedConnection>,
) -> Option<WorkerMessage> {
    if cached.is_some() {
        match receiver.recv_timeout(IDLE_CACHE_TTL) {
            Ok(message) => return Some(message),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Drop the cached connection after an idle period so file locks
                // and descriptors are released once traffic stops, while keeping
                // the worker thread parked.
                *cached = None;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    receiver.recv().ok()
}

#[cfg(not(unix))]
fn recv_next_task(
    receiver: &std::sync::mpsc::Receiver<WorkerMessage>,
    _cached: &mut Option<CachedConnection>,
) -> Option<WorkerMessage> {
    receiver.recv().ok()
}

fn serve_connection(
    runtime: &tokio::runtime::Runtime,
    task: OpenTask,
    cached: Option<CachedConnection>,
) -> Served {
    let OpenTask {
        filename,
        flags,
        timeout_ms,
        ready,
        mut requests,
    } = task;
    let Some(open_flags) = OpenFlags::from_bits(flags) else {
        drop(cached);
        let _ = ready.send(Err(Error {
            code: SQLITE_MISUSE,
            message: "bad parameter or other API misuse".into(),
        }));
        return Served::Clean;
    };

    #[cfg(unix)]
    let (mut connection, file_id) = {
        let mut reused: Option<(Rc<Connection>, (u64, u64))> = None;
        if let Some(entry) = cached
            && entry.filename == filename
            && entry.flags == flags
            && file_identity(&filename).is_some_and(|id| id == entry.file_id)
        {
            let ok = if entry.timeout_ms != timeout_ms {
                entry
                    .connection
                    .busy_timeout(Duration::from_millis(timeout_ms))
                    .is_ok()
            } else {
                true
            };
            if ok {
                reused = Some((entry.connection, entry.file_id));
            }
        }
        if let Some((conn, id)) = reused {
            register_active_file(id);
            (conn, Some(id))
        } else {
            let opened = Connection::open_with_flags(&filename, open_flags)
                .map_err(Error::from)
                .and_then(|conn| {
                    conn.busy_timeout(Duration::from_millis(timeout_ms))?;
                    Ok(conn)
                });
            match opened {
                Ok(conn) => {
                    let id = if cacheable_path(&filename, open_flags) {
                        file_identity(&filename)
                    } else {
                        None
                    };
                    if let Some(id) = id {
                        register_active_file(id);
                    }
                    (Rc::new(conn), id)
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return Served::Clean;
                }
            }
        }
    };

    #[cfg(not(unix))]
    let mut connection = {
        drop(cached);
        let opened = Connection::open_with_flags(&filename, open_flags)
            .map_err(Error::from)
            .and_then(|conn| {
                conn.busy_timeout(Duration::from_millis(timeout_ms))?;
                Ok(conn)
            });
        match opened {
            Ok(conn) => Rc::new(conn),
            Err(error) => {
                let _ = ready.send(Err(error));
                return Served::Clean;
            }
        }
    };

    let _ = ready.send(Ok(()));
    let dirty = Rc::new(Cell::new(false));
    let local = tokio::task::LocalSet::new();
    let closed = local.block_on(runtime, async {
        while let Some(request) = requests.recv().await {
            match request {
                Request::Call { operation, mutates } => {
                    if mutates {
                        dirty.set(true);
                    }
                    operation(&mut connection);
                }
                Request::Prepare {
                    database,
                    sql,
                    ready,
                } => {
                    let conn = connection.clone();
                    let dirty_flag = dirty.clone();
                    let lifetime = std::sync::Arc::new(crate::lifetime::CursorTask {
                        database,
                        task: std::sync::OnceLock::new(),
                    });
                    let cursor_lifetime = lifetime.clone();
                    let task = tokio::task::spawn_local(async move {
                        crate::cursor::serve(conn, cursor_lifetime, sql, ready, dirty_flag).await;
                    });
                    let _ = lifetime.task.set(task);
                }
                Request::Close(reply) => return Some(reply),
                Request::FinishCursor(task) => {
                    task.abort();
                    let _ = task.await;
                }
            }
        }
        None
    });
    // Finalize all cursors before acknowledging close() or deciding cache eligibility.
    drop(local);

    #[cfg(unix)]
    if let Some(id) = file_id {
        let can_cache = closed.is_none()
            && !dirty.get()
            && Rc::strong_count(&connection) == 1
            && connection.is_autocommit()
            && connection.changes() == 0
            && connection.last_insert_rowid() == 0
            && cacheable_path(&filename, open_flags);
        match release_and_maybe_park(id, can_cache, &filename, flags) {
            ReleaseOutcome::ParkedCached(receiver) => {
                return Served::ParkedCached(
                    CachedConnection {
                        filename,
                        flags,
                        timeout_ms,
                        file_id: id,
                        connection,
                    },
                    receiver,
                );
            }
            ReleaseOutcome::PoolFull => {
                drop(connection);
                if let Some(reply) = closed {
                    let _ = reply.send(());
                }
                return Served::Exit;
            }
            ReleaseOutcome::DropAndEvict(evict_senders) => {
                // Evict all cached peers first so this final connection closes
                // alone without contending on SQLite's non-blocking WAL-close
                // exclusive lock (which checkpoints and removes -wal/-shm).
                evict_workers(evict_senders);
                drop(connection);
                if let Some(reply) = closed {
                    let _ = reply.send(());
                }
                return Served::Clean;
            }
        }
    }

    drop(connection);
    if let Some(reply) = closed {
        let _ = reply.send(());
    }
    let _ = (&filename, open_flags, timeout_ms, dirty);
    Served::Clean
}

#[derive(Clone)]
pub struct Database {
    sender: mpsc::UnboundedSender<Request>,
}

impl Database {
    pub async fn close(&self) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Request::Close(sender))
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())
    }

    pub async fn open(filename: String, flags: i32, timeout_ms: u64) -> Result<Self, Error> {
        let (sender, requests) = mpsc::unbounded_channel::<Request>();
        let (ready, opened) = oneshot::channel();
        let mut task = OpenTask {
            filename,
            flags,
            timeout_ms,
            ready,
            requests,
        };
        while let Some(worker) = pop_idle_worker(&task.filename, task.flags) {
            match worker.sender.send(WorkerMessage::Open(task)) {
                Ok(()) => {
                    opened.await.map_err(|_| Error::closed())??;
                    return Ok(Self { sender });
                }
                Err(std::sync::mpsc::SendError(WorkerMessage::Open(returned))) => {
                    task = returned;
                }
                #[cfg(unix)]
                Err(std::sync::mpsc::SendError(WorkerMessage::Evict(_))) => unreachable!(),
            }
        }
        // Even thread creation is moved off the PHP worker.
        tokio::task::spawn_blocking(move || {
            std::thread::Builder::new()
                .name("phera-sqlite".into())
                .spawn(move || run_worker(task))
                .map_err(|error| Error {
                    code: SQLITE_ERROR,
                    message: error.to_string(),
                })
        })
        .await
        .map_err(|_| Error::closed())??;
        opened.await.map_err(|_| Error::closed())??;
        Ok(Self { sender })
    }

    /// Runs `operation` with the connection on the database's thread and
    /// waits for its result, without blocking the PHP worker.
    pub async fn call<T, F>(&self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, Error> + Send + 'static,
    {
        self.run(true, move |connection| operation(connection))
            .await
    }

    async fn run<T, F>(&self, mutates: bool, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Rc<Connection>) -> Result<T, Error> + Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Request::Call {
                operation: Box::new(move |connection| {
                    let _ = sender.send(operation(connection));
                }),
                mutates,
            })
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
    }

    pub async fn prepare(&self, sql: String) -> Result<Statement, Error> {
        let (sender, receiver) = oneshot::channel();
        let database = self.clone();
        self.sender
            .send(Request::Prepare {
                database,
                sql,
                ready: sender,
            })
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
    }

    pub(crate) fn finish_cursor(&self, task: tokio::task::JoinHandle<()>) {
        let _ = self.sender.send(Request::FinishCursor(task));
    }

    /// Like `call`, with the connection mutable, which a backup needs; that
    /// is only possible while no statement of this database is open.
    pub async fn exclusive<T, F>(&self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    {
        self.run(true, move |connection| match Rc::get_mut(connection) {
            Some(connection) => operation(connection),
            None => Err(Error {
                code: SQLITE_LOCKED,
                message: "destination has outstanding prepared statements".into(),
            }),
        })
        .await
    }

    pub fn same_connection(&self, other: &Self) -> bool {
        self.sender.same_channel(&other.sender)
    }

    pub async fn exec(&self, sql: String) -> Result<u64, Error> {
        self.call(move |connection| {
            connection.execute_batch(&sql)?;
            Ok(connection.changes())
        })
        .await
    }

    pub async fn last_id(&self) -> Result<i64, Error> {
        self.run(false, |connection| Ok(connection.last_insert_rowid()))
            .await
    }

    pub async fn changes(&self) -> Result<u64, Error> {
        self.run(false, |connection| Ok(connection.changes())).await
    }

    pub async fn transaction(&self) -> Result<bool, Error> {
        self.run(false, |connection| Ok(!connection.is_autocommit()))
            .await
    }

    pub async fn timeout(&self, milliseconds: u64) -> Result<(), Error> {
        self.call(move |connection| {
            connection.busy_timeout(Duration::from_millis(milliseconds))?;
            Ok(())
        })
        .await
    }
}
