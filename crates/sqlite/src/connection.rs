//! `exec('BEGIN; INSERT ...; COMMIT')` runs entirely on the database's owning thread.

use std::rc::Rc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::{mpsc, oneshot};

use crate::data::{SQLITE_ERROR, SQLITE_LOCKED, SQLITE_MISUSE};
use crate::{Error, Statement};

type Operation = Box<dyn FnOnce(&mut Rc<Connection>) + Send>;
enum Request {
    Call(Operation),
    Close(oneshot::Sender<()>),
    FinishCursor(tokio::task::JoinHandle<()>),
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
        let (sender, mut receiver) = mpsc::unbounded_channel::<Request>();
        let (ready, opened) = oneshot::channel();
        // Even thread creation is moved off the PHP worker.
        tokio::task::spawn_blocking(move || {
            std::thread::Builder::new()
                .name("phera-sqlite".into())
                .spawn(move || {
                    let connection = OpenFlags::from_bits(flags)
                        .ok_or_else(|| Error {
                            code: SQLITE_MISUSE,
                            message: "bad parameter or other API misuse".into(),
                        })
                        .and_then(|flags| {
                            Connection::open_with_flags(filename, flags).map_err(Error::from)
                        })
                        .and_then(|connection| {
                            connection.busy_timeout(Duration::from_millis(timeout_ms))?;
                            Ok(connection)
                        });
                    let mut connection = match connection {
                        Ok(connection) => Rc::new(connection),
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                    let runtime = match tokio::runtime::Builder::new_current_thread().build() {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ = ready.send(Err(Error {
                                code: SQLITE_ERROR,
                                message: error.to_string(),
                            }));
                            return;
                        }
                    };
                    let _ = ready.send(Ok(()));
                    let local = tokio::task::LocalSet::new();
                    let closed = local.block_on(&runtime, async move {
                        while let Some(request) = receiver.recv().await {
                            match request {
                                Request::Call(operation) => operation(&mut connection),
                                Request::Close(reply) => return Some(reply),
                                Request::FinishCursor(task) => {
                                    task.abort();
                                    let _ = task.await;
                                }
                            }
                        }
                        None
                    });
                    // Finalize all cursors and close the database before acknowledging close().
                    drop(local);
                    if let Some(reply) = closed {
                        let _ = reply.send(());
                    }
                })
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
        self.run(move |connection| operation(connection)).await
    }

    async fn run<T, F>(&self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Rc<Connection>) -> Result<T, Error> + Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Request::Call(Box::new(move |connection| {
                let _ = sender.send(operation(connection));
            })))
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
    }

    pub async fn prepare(&self, sql: String) -> Result<Statement, Error> {
        let (sender, receiver) = oneshot::channel();
        let database = self.clone();
        self.sender
            .send(Request::Call(Box::new(move |connection| {
                let connection = connection.clone();
                let lifetime = std::sync::Arc::new(crate::lifetime::CursorTask {
                    database,
                    task: std::sync::OnceLock::new(),
                });
                let cursor_lifetime = lifetime.clone();
                let task = tokio::task::spawn_local(async move {
                    crate::cursor::serve(connection, cursor_lifetime, sql, sender).await;
                });
                let _ = lifetime.task.set(task);
            })))
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
        self.run(move |connection| match Rc::get_mut(connection) {
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
        self.call(|connection| Ok(connection.last_insert_rowid()))
            .await
    }

    pub async fn changes(&self) -> Result<u64, Error> {
        self.call(|connection| Ok(connection.changes())).await
    }

    pub async fn transaction(&self) -> Result<bool, Error> {
        self.call(|connection| Ok(!connection.is_autocommit()))
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
