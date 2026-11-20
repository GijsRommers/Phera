//! An unbuffered MySQL statement reads rows from MySQL only when PHP fetches them.

use std::sync::{Arc, Weak};

use mysql_async::prelude::{Protocol, Queryable};
use mysql_async::{Column, Conn, Params, QueryResult, Row, Statement};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::{
    Mysql, error,
    native::{Outcome, Reply},
};
use crate::drivers::RawRows;
use crate::errors::DbError;

pub(super) enum Query {
    Text(Vec<u8>),
    Binary(Statement, Params),
}

struct Request {
    all: bool,
    result: oneshot::Sender<Result<Batch, DbError>>,
}

pub(super) struct Active {
    owner: Weak<()>,
    requests: Option<mpsc::Sender<Request>>,
    connection: Option<oneshot::Receiver<Conn>>,
    task: JoinHandle<()>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Batch {
    columns: Vec<Column>,
    rows: Vec<Row>,
    done: bool,
}

struct Header {
    columns: Vec<Column>,
    reply: Reply,
}

impl Mysql {
    pub(super) async fn abort_stream(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.task.abort();
            let _ = (&mut active.task).await;
            if let Some(returned) = active.connection.take()
                && let Ok(connection) = returned.await
            {
                self.connection = Some(connection);
            }
        }
    }

    pub(super) async fn stream(
        &mut self,
        query: Query,
        owner: &Arc<()>,
    ) -> Result<Outcome, DbError> {
        let connection = self.connection.take().ok_or_else(Mysql::closed)?;
        let (requests, pending) = mpsc::channel(1);
        let (initial, header) = oneshot::channel();
        let (finished, remaining) = oneshot::channel();
        let task = tokio::spawn(run(connection, query, pending, initial, finished));
        self.active = Some(Active {
            owner: Arc::downgrade(owner),
            requests: Some(requests),
            connection: Some(remaining),
            task,
        });
        let header = header
            .await
            .map_err(|_| Mysql::closed())
            .and_then(|result| result);
        let header = match header {
            Ok(header) => header,
            Err(error) => {
                self.finish_stream().await;
                return Err(error);
            }
        };
        if header.columns.is_empty() {
            self.finish_stream().await;
        }
        self.last_insert_id = header.reply.insert_id;
        Ok(Outcome {
            rows: RawRows::Mysql {
                columns: header.columns,
                rows: Vec::new(),
                affected: header.reply.affected,
            },
            reply: header.reply,
        })
    }

    pub async fn read_stream(
        &mut self,
        owner: &Arc<()>,
        all: bool,
    ) -> Result<Option<(RawRows, bool)>, DbError> {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| active.belongs_to(owner))
        else {
            return Ok(None);
        };
        let (result, batch) = oneshot::channel();
        let sent = match &active.requests {
            Some(sender) => sender.send(Request { all, result }).await.is_ok(),
            None => false,
        };
        let batch = if sent {
            batch
                .await
                .map_err(|_| Mysql::closed())
                .and_then(|result| result)
        } else {
            Err(Mysql::closed())
        };
        if !matches!(&batch, Ok(batch) if !batch.done) {
            self.finish_stream().await;
        }
        let Batch {
            columns,
            rows,
            done,
        } = batch?;
        Ok(Some((
            RawRows::Mysql {
                columns,
                rows,
                affected: 0,
            },
            done,
        )))
    }

    pub async fn close_stream(&mut self, owner: &Arc<()>) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.belongs_to(owner))
        {
            self.finish_stream().await;
        }
    }

    pub(super) async fn finish_stream(&mut self) {
        if let Some(mut active) = self.active.take() {
            drop(active.requests.take());
            if let Some(connection) = active.connection.take() {
                self.connection = connection.await.ok();
            }
        }
    }
}

impl Active {
    fn belongs_to(&self, owner: &Arc<()>) -> bool {
        Weak::ptr_eq(&self.owner, &Arc::downgrade(owner))
    }

    pub(super) fn abandoned(&self) -> bool {
        self.owner.strong_count() == 0
    }
}

async fn run(
    mut connection: Conn,
    query: Query,
    pending: mpsc::Receiver<Request>,
    initial: oneshot::Sender<Result<Header, DbError>>,
    finished: oneshot::Sender<Conn>,
) {
    match query {
        Query::Text(sql) => read(connection.query_iter(sql).await, pending, initial).await,
        Query::Binary(statement, params) => {
            read(
                connection.exec_iter(statement, params).await,
                pending,
                initial,
            )
            .await;
        }
    }
    let _ = finished.send(connection);
}

async fn read<P: Protocol>(
    result: mysql_async::Result<QueryResult<'_, '_, P>>,
    mut pending: mpsc::Receiver<Request>,
    initial: oneshot::Sender<Result<Header, DbError>>,
) {
    let mut result = match result {
        Ok(result) => result,
        Err(failure) => {
            let _ = initial.send(Err(error(failure)));
            return;
        }
    };
    let columns = result.columns_ref().to_vec();
    let header = Header {
        columns: columns.clone(),
        reply: Reply {
            affected: result.affected_rows(),
            insert_id: result.last_insert_id().unwrap_or_default(),
            info: result.info().into_owned(),
            warnings: result.warnings(),
        },
    };
    if columns.is_empty() {
        let _ = initial.send(result.drop_result().await.map(|()| header).map_err(error));
        return;
    }
    if initial.send(Ok(header)).is_ok() {
        while let Some(request) = pending.recv().await {
            let batch = read_batch(&mut result, &columns, request.all).await;
            let done = !matches!(&batch, Ok(batch) if !batch.done);
            if request.result.send(batch).is_err() || done {
                break;
            }
        }
    }
    let _ = result.drop_result().await;
}

async fn read_batch<P: Protocol>(
    result: &mut QueryResult<'_, '_, P>,
    columns: &[Column],
    all: bool,
) -> Result<Batch, DbError> {
    let (rows, done) = if all {
        (result.collect().await.map_err(error)?, true)
    } else {
        match result.next().await.map_err(error)? {
            Some(row) => (vec![row], false),
            None => (Vec::new(), true),
        }
    };
    Ok(Batch {
        columns: columns.to_vec(),
        rows,
        done,
    })
}
