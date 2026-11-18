//! Live cursors: `$s->execute(); $s->fetch()` steps once per fetch and retains read locks.
//! The statement borrows its connection inside a local task on the database thread.

use crate::{Cell, Description, Error, Execution};
use rusqlite::{Connection, Rows};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};

type Reply<T> = oneshot::Sender<Result<T, Error>>;

enum Command {
    Execute(Vec<(usize, Cell)>, Reply<Execution>),
    Bind(Vec<(usize, Cell)>, Reply<()>),
    Fetch(Reply<Option<Vec<Cell>>>),
    Reset(Reply<()>),
    Clear(Reply<()>),
    Sql(Reply<Option<String>>),
}

#[derive(Clone)]
pub struct Statement {
    sender: mpsc::UnboundedSender<Command>,
    pub description: Description,
    busy: Arc<AtomicBool>,
    // Keeps the connection actor alive until the last statement handle disappears.
    _lifetime: Arc<crate::lifetime::CursorTask>,
}

impl Statement {
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    pub async fn bind(&self, bindings: Vec<(usize, Cell)>) -> Result<(), Error> {
        self.ask(|reply| Command::Bind(bindings, reply)).await
    }

    /// Runs the statement and returns its first row, if any; the cursor
    /// stays open for `fetch` until the last row.
    pub async fn execute(&self, bindings: Vec<(usize, Cell)>) -> Result<Execution, Error> {
        self.ask(|reply| Command::Execute(bindings, reply)).await
    }

    pub async fn fetch(&self) -> Result<Option<Vec<Cell>>, Error> {
        self.ask(Command::Fetch).await
    }

    pub async fn reset(&self) -> Result<(), Error> {
        self.ask(Command::Reset).await
    }

    pub async fn clear(&self) -> Result<(), Error> {
        self.ask(Command::Clear).await
    }

    pub async fn expanded_sql(&self) -> Result<Option<String>, Error> {
        self.ask(Command::Sql).await
    }

    /// Sends a command to the statement's task on the database thread and
    /// waits for its reply. A closed database answers `Error::closed()`.
    async fn ask<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Error> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(command(sender))
            .map_err(|_| Error::closed())?;
        receiver.await.map_err(|_| Error::closed())?
    }
}

pub(crate) async fn serve(
    connection: Rc<Connection>,
    lifetime: Arc<crate::lifetime::CursorTask>,
    sql: String,
    ready: Reply<Statement>,
) {
    let mut statement = match connection.prepare(&sql) {
        Ok(statement) => statement,
        Err(error) => {
            let _ = ready.send(Err(error.into()));
            return;
        }
    };
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let busy = Arc::new(AtomicBool::new(false));
    let handle = Statement {
        busy: busy.clone(),
        sender,
        description: Description::read(&statement),
        _lifetime: lifetime,
    };
    if ready.send(Ok(handle)).is_err() {
        return;
    }
    let mut pending = None;
    loop {
        let command = match pending.take() {
            Some(command) => command,
            None => match receiver.recv().await {
                Some(command) => command,
                None => break,
            },
        };
        busy.store(false, Ordering::Release);
        match command {
            Command::Execute(bindings, reply) => {
                let bound = bindings
                    .into_iter()
                    .try_for_each(|(index, value)| statement.raw_bind_parameter(index, value));
                if let Err(error) = bound {
                    let _ = reply.send(Err(error.into()));
                    continue;
                }
                let description = Description::read(&statement);
                let mut rows = statement.raw_query();
                let first = read(&mut rows);
                let active = matches!(&first, Ok(Some(_)));
                busy.store(active, Ordering::Release);
                let result = first.map(|first| Execution {
                    description,
                    first,
                    changes: connection.changes(),
                });
                let _ = reply.send(result);
                if !active {
                    continue;
                }
                // Dropping Rows resets SQLite. Keep it alive between fetch commands.
                while let Some(command) = receiver.recv().await {
                    match command {
                        Command::Fetch(reply) => {
                            let result = read(&mut rows);
                            let finished = !matches!(&result, Ok(Some(_)));
                            busy.store(!finished, Ordering::Release);
                            let _ = reply.send(result);
                            if finished {
                                break;
                            }
                        }
                        Command::Sql(reply) => {
                            let _ = reply.send(Ok(rows.as_ref().and_then(|s| s.expanded_sql())));
                        }
                        command => {
                            pending = Some(command);
                            break;
                        }
                    }
                }
            }
            Command::Bind(bindings, reply) => {
                let result = bindings
                    .into_iter()
                    .try_for_each(|(index, value)| statement.raw_bind_parameter(index, value))
                    .map_err(Error::from);
                let _ = reply.send(result);
            }
            Command::Fetch(reply) => {
                let _ = reply.send(Ok(None));
            }
            Command::Reset(reply) => {
                let _ = reply.send(Ok(()));
            }
            Command::Clear(reply) => {
                statement.clear_bindings();
                let _ = reply.send(Ok(()));
            }
            Command::Sql(reply) => {
                let _ = reply.send(Ok(statement.expanded_sql()));
            }
        }
    }
}

fn read(rows: &mut Rows<'_>) -> Result<Option<Vec<Cell>>, Error> {
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let values = (0..row.as_ref().column_count())
        .map(|index| row.get_ref(index).map(Cell::from_ref))
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(values))
}
