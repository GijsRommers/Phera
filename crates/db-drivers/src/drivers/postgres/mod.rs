//! PostgreSQL requests text cells over the SQL socket on the PHP worker.

mod connect;
mod information;
mod lazy;
mod metadata;
mod native;
mod stream;
mod tls;
mod values;
mod verify;
mod wire;

pub use native::{NativeColumn, NativeResult, NativeSequence};
pub use wire::Notification;

#[cfg(test)]
mod tls_tests;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use phera_extension_api::Value;
use postgres_protocol::message::backend::Message;

use super::{Execution, Prepared, RowBuffer, Rows, connection_error};
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::{Sql, Syntax};

pub struct Postgres {
    client: wire::Client,
    pub version: String,
    lazy: Option<lazy::Active>,
    native: Option<native::State>,
    statements: Vec<OpenStatement>,
}

/// A PDO statement gets a name before its first Parse is sent.
#[derive(Clone)]
pub struct Statement(Rc<PdoStatement>);

struct PdoStatement {
    name: String,
    parsed: Cell<bool>,
    owner: Arc<()>,
}

struct OpenStatement {
    name: String,
    owner: Weak<()>,
}

impl Statement {
    fn new(number: u64) -> Self {
        Self(Rc::new(PdoStatement {
            name: format!("pdo_stmt_{number:08x}"),
            parsed: Cell::new(false),
            owner: Arc::new(()),
        }))
    }
}

impl Postgres {
    pub fn transaction_state(&self) -> bool {
        !self.is_broken() && self.client.status() != b'I'
    }

    pub async fn close(self) {
        // Dropping the socket rolls back any transaction on the server.
        drop(self);
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        self.discard_lazy().await?;
        if self.transaction_state() {
            self.run_unprepared("ROLLBACK").await?;
        }
        self.run_unprepared("DISCARD ALL").await?;
        self.statements.clear();
        self.client.clear_messages();
        Ok(())
    }

    pub fn is_broken(&self) -> bool {
        self.client.is_closed()
    }

    pub fn reusable(&self) -> bool {
        !self.is_broken() && !self.native.as_ref().is_some_and(native::State::copying)
    }

    pub async fn health(&mut self) -> Result<(), DbError> {
        self.run_unprepared("SELECT 1").await.map(|_| ())
    }

    pub fn prepare(&self, number: u64) -> Statement {
        Statement::new(number)
    }

    /// Release connections with long-lived PDO handles from retaining every
    /// statement ever executed. Close dead names in the next first execution.
    fn closed_statements(&mut self) -> Vec<String> {
        let mut closed = Vec::new();
        self.statements.retain_mut(|statement| {
            if statement.owner.strong_count() == 0 {
                closed.push(std::mem::take(&mut statement.name));
                false
            } else {
                true
            }
        });
        closed
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        parameters: &[BoundValue],
        options: &Execution,
    ) -> Result<Rows, DbError> {
        self.discard_lazy().await?;
        let owner = options.postgres_stream.as_ref();
        if matches!(prepared, Prepared::Emulated) {
            let query = if parameters.is_empty() {
                sql.unbound_sql().into_owned()
            } else {
                sql.check_emulation()?;
                let backslash_strings = self.backslash_strings();
                sql.check_string_mode(Syntax::Postgres { backslash_strings })?;
                let literals: Vec<_> = parameters
                    .iter()
                    .map(|value| match value {
                        BoundValue::String(bytes) => quote_string(bytes, backslash_strings),
                        value => literal(value, false),
                    })
                    .collect();
                sql.render(|index| literals[index].clone())
            };
            self.client.simple(&query).await?;
            return self.read_pdo(query, owner, None).await;
        }
        let query = sql.render(|i| format!("${}", i + 1));
        let temporary;
        let statement = match prepared {
            Prepared::Postgres { statement, .. } => Some(&*statement.0),
            Prepared::PostgresDeferred { number } => {
                temporary = Statement::new(*number);
                Some(&*temporary.0)
            }
            Prepared::PostgresUnnamed => None,
            _ => return Err(connection_error("Mismatched database driver")),
        };
        let first = statement.is_none_or(|statement| !statement.parsed.get());
        let close = if first {
            self.closed_statements()
        } else {
            Vec::new()
        };
        let name = statement.map_or("", |statement| statement.name.as_str());
        self.client
            .extended(name, first.then_some(query.as_str()), parameters, &close)
            .await?;
        self.read_pdo(query, owner, statement).await
    }

    async fn read_pdo(
        &mut self,
        query: String,
        owner: Option<&Arc<()>>,
        statement: Option<&PdoStatement>,
    ) -> Result<Rows, DbError> {
        let mut rows = Rows::default();
        let mut described = false;
        loop {
            match self.client.next().await? {
                Message::RowDescription(body) => {
                    described = true;
                    rows.columns = wire::rows::fields(&body)
                        .map_err(|error| self.client.invalid(error))?
                        .into_iter()
                        .map(values::column)
                        .collect();
                    rows.values = RowBuffer::with_capacity(rows.columns.len(), 0);
                }
                Message::DataRow(body) => {
                    if let Some(owner) = owner {
                        self.lazy =
                            Some(lazy::Active::new(owner, rows.columns.clone(), query, body));
                        rows.affected = 1;
                        rows.has_pending_rows = true;
                        return Ok(rows);
                    }
                    if let Err(error) = values::append(&rows.columns, &mut rows.values, &body) {
                        if error.state == "08006" {
                            return Err(self.client.invalid(error.detail));
                        }
                        let _ = self.finish(&query).await;
                        return Err(error);
                    }
                }
                Message::ParseComplete => {
                    if let Some(statement) = statement {
                        statement.parsed.set(true);
                        self.statements.push(OpenStatement {
                            name: statement.name.clone(),
                            owner: Arc::downgrade(&statement.owner),
                        });
                    }
                }
                Message::CommandComplete(body) => {
                    if described {
                        rows.affected = rows.values.len() as u64;
                    } else {
                        rows = Rows {
                            affected: affected(
                                body.tag().map_err(|error| self.client.invalid(error))?,
                            ),
                            ..Rows::default()
                        };
                    }
                    described = false;
                    if owner.is_some() {
                        self.finish_after_result(&query).await?;
                        return Ok(rows);
                    }
                }
                Message::ReadyForQuery(_) => return Ok(rows),
                Message::ErrorResponse(error) => {
                    return Err(self.client.failure(error, &query, false).await);
                }
                Message::BindComplete
                | Message::CloseComplete
                | Message::NoData
                | Message::EmptyQueryResponse => {}
                _ => return Err(self.client.invalid("unexpected PostgreSQL query response")),
            }
        }
    }

    pub async fn run_unprepared(&mut self, sql: &str) -> Result<Rows, DbError> {
        self.client.simple(sql).await?;
        let affected = self.finish(sql).await?;
        Ok(Rows {
            affected,
            ..Rows::default()
        })
    }

    /// Discard result data and leave the socket at ReadyForQuery.
    async fn finish(&mut self, query: &str) -> Result<u64, DbError> {
        let mut count = 0;
        let mut returned_rows = false;
        loop {
            match self.client.next().await? {
                Message::RowDescription(_) => returned_rows = true,
                Message::CommandComplete(body) => {
                    count = if returned_rows {
                        0
                    } else {
                        affected(body.tag().map_err(|error| self.client.invalid(error))?)
                    };
                    returned_rows = false;
                }
                Message::ReadyForQuery(_) => return Ok(count),
                Message::ErrorResponse(error) => {
                    return Err(self.client.failure(error, query, false).await);
                }
                Message::CopyInResponse(_) => {
                    return Err(self
                        .client
                        .invalid("COPY requires a PostgreSQL COPY operation"));
                }
                _ => {}
            }
        }
    }

    /// A streamed query exposes its first result. Drain later commands when
    /// emulation sent several statements in one simple Query message.
    async fn finish_after_result(&mut self, query: &str) -> Result<(), DbError> {
        match self.finish(query).await {
            Err(error) if self.is_broken() => Err(error),
            _ => Ok(()),
        }
    }

    pub fn quote(&self, bytes: &[u8]) -> String {
        quote_string(bytes, self.backslash_strings())
    }

    pub fn backslash_strings(&self) -> bool {
        self.client.parameter("standard_conforming_strings") == Some("off")
    }

    pub async fn last_id(&mut self, name: Option<&str>) -> Result<String, DbError> {
        let (query, values) = match name {
            Some(name) => (
                "SELECT currval($1::text)",
                vec![BoundValue::String(name.as_bytes().to_vec())],
            ),
            None => ("SELECT lastval()", Vec::new()),
        };
        self.client.extended("", Some(query), &values, &[]).await?;
        let result = self.read_pdo(query.into(), None, None).await?;
        match result.values.get(0).and_then(|row| row.first()) {
            Some(Value::Int(id)) => Ok(id.to_string()),
            _ => Err(connection_error(
                "PostgreSQL did not return a sequence value",
            )),
        }
    }
}

fn affected(tag: &str) -> u64 {
    tag.rsplit(' ')
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or_default()
}

fn quote_string(bytes: &[u8], backslash_strings: bool) -> String {
    let quoted = quote(bytes, false);
    if backslash_strings {
        quoted.replace('\\', "\\\\")
    } else {
        quoted
    }
}
