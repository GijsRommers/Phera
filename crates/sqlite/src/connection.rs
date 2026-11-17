//! `exec('BEGIN; INSERT ...; COMMIT')` runs entirely on the database's owning thread.

use std::rc::Rc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::{mpsc, oneshot};

use crate::{Error, Statement};

type Operation = Box<dyn FnOnce(&Rc<Connection>) + Send>;

#[derive(Clone)]
pub struct Database {
    sender: mpsc::UnboundedSender<Operation>,
}

impl Database {
    pub async fn open(filename: String, flags: i32, timeout_ms: u64) -> Result<Self, Error> {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Operation>();
        let (ready, opened) = oneshot::channel();
        // Even thread creation is moved off the PHP worker.
        tokio::task::spawn_blocking(move || {
            std::thread::Builder::new()
                .name("phera-sqlite".into())
                .spawn(move || {
                    let connection = OpenFlags::from_bits(flags)
                        .ok_or_else(|| Error {
                            code: 21,
                            message: "bad parameter or other API misuse".into(),
                        })
                        .and_then(|flags| {
                            Connection::open_with_flags(filename, flags).map_err(Error::from)
                        })
                        .and_then(|connection| {
                            connection.busy_timeout(Duration::from_millis(timeout_ms))?;
                            Ok(connection)
                        });
                    let connection = match connection {
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
                                code: 1,
                                message: error.to_string(),
                            }));
                            return;
                        }
                    };
                    let _ = ready.send(Ok(()));
                    tokio::task::LocalSet::new().block_on(&runtime, async move {
                        while let Some(operation) = receiver.recv().await {
                            operation(&connection);
                        }
                    });
                    // Local tasks (and their cursors) are dropped before the connection.
                })
                .map_err(|error| Error {
                    code: 1,
                    message: error.to_string(),
                })
        })
        .await
        .map_err(|_| Error::closed())??;
        opened.await.map_err(|_| Error::closed())??;
        Ok(Self { sender })
    }

    pub async fn call<T, F>(&self, operation: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, Error> + Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Box::new(move |connection| {
                let _ = sender.send(operation(connection));
            }))
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
    }

    pub async fn prepare(&self, sql: String) -> Result<Statement, Error> {
        let (sender, receiver) = oneshot::channel();
        let database = self.clone();
        self.sender
            .send(Box::new(move |connection| {
                let connection = connection.clone();
                tokio::task::spawn_local(async move {
                    crate::cursor::serve(connection, database, sql, sender).await;
                });
            }))
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
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
