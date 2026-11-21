//! PostgreSQL through tokio-postgres. PDO's `?` and `:name` placeholders
//! become `$1`, `$2` on the wire:
//!
//! ```php
//! $statement = $pdo->prepare('SELECT ?::int AS a, :b::text AS b');
//! $statement->execute([1, 'b' => 'two']); // sent as SELECT $1::int AS a, $2::text AS b
//! ```

mod connect;
mod information;
mod lazy;
mod metadata;
mod native;
mod observations;
pub use native::{NativeColumn, NativeResult, NativeSequence};
mod parameters;
mod protocol;
mod stream;
mod text;
mod tls;
mod unnamed;
pub mod values;
mod verify;

#[cfg(test)]
mod tls_tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use protocol::TextParameter;
pub use protocol::error;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, SimpleQueryMessage, Statement};

use super::{Column, Execution, Prepared, RawRows, connection_error};
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::{Sql, Syntax};

/// The status byte of PostgreSQL's last ReadyForQuery message: `I` idle,
/// `T` in a transaction, `E` in a failed transaction.
const IDLE: u8 = b'I';

/// One row of a result: its values in PostgreSQL's binary format, and the
/// ones PostgreSQL formatted as text for PDO, by column index.
pub struct Row {
    binary: tokio_postgres::Row,
    text: Vec<(usize, String)>,
}

pub struct Postgres {
    client: Client,
    /// Drives the socket; tokio-postgres sends and receives through it.
    task: tokio::task::JoinHandle<()>,
    pub version: String,
    /// The latest ReadyForQuery status, kept up to date by the socket.
    transaction: Arc<AtomicU8>,
    /// Set once the connection may hold type information that `DISCARD ALL`
    /// cannot clear; see `prepare_query`.
    retire: bool,
    lazy: Option<lazy::Active>,
    native: Option<native::State>,
}

impl Drop for Postgres {
    fn drop(&mut self) {
        // Closing the socket causes PostgreSQL to roll back any open transaction.
        self.task.abort();
    }
}

impl Postgres {
    pub async fn transaction_state(&self) -> bool {
        // An error may be delivered before the connection task consumes its
        // ReadyForQuery. An empty query is valid even in a failed transaction
        // and acts as a protocol barrier when the PHP script asks for state.
        if self.lazy.is_none() {
            let _ = self.client.simple_query("").await;
        }
        !self.client.is_closed() && self.transaction.load(Ordering::Acquire) != IDLE
    }

    pub async fn metadata(&self, column: &Column) -> Result<(i16, Option<String>), DbError> {
        metadata::get(&self.client, column).await
    }

    pub async fn close(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        self.discard_lazy();
        if let Some(native) = &mut self.native {
            native.reset();
        }
        // Separate protocol messages: DISCARD ALL cannot execute in a transaction.
        self.client
            .batch_execute("ROLLBACK")
            .await
            .map_err(|e| error(e, "ROLLBACK"))?;
        self.client
            .batch_execute("DISCARD ALL")
            .await
            .map_err(|e| error(e, "DISCARD ALL"))?;
        if let Some(native) = &mut self.native {
            // ROLLBACK can emit a notice even for a clean session.
            native.reset();
        }
        Ok(())
    }

    pub fn is_broken(&self) -> bool {
        self.client.is_closed()
    }

    pub async fn health(&self) -> Result<(), DbError> {
        self.client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| error(e, "SELECT 1"))?;
        Ok(())
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Statement, DbError> {
        self.discard_lazy();
        let query = sql.render(|i| format!("${}", i + 1));
        self.prepare_query(&query).await
    }

    async fn prepare_query(&mut self, query: &str) -> Result<Statement, DbError> {
        let result = self
            .client
            .prepare(query)
            .await
            .map_err(|e| error(e, query));
        // tokio-postgres retains private type-info prepared statements which its
        // public clear_type_cache() cannot forget. DISCARD ALL invalidates them.
        // Retire clients that may have populated those caches rather than lend
        // a client with stale hidden statements to the next script.
        let needs_fresh_client = match &result {
            Ok(statement) => statement
                .params()
                .iter()
                .chain(statement.columns().iter().map(|column| column.type_()))
                .any(|kind| tokio_postgres::types::Type::from_oid(kind.oid()).is_none()),
            Err(_) => true,
        };
        if needs_fresh_client {
            self.retire = true;
        }
        result
    }

    pub fn reusable(&self) -> bool {
        !self.retire
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: &Execution,
    ) -> Result<RawRows, DbError> {
        self.discard_lazy();
        let statement = match prepared {
            Prepared::Postgres { statement, .. } => statement.clone(),
            Prepared::PostgresDeferred { .. } => self.prepare(sql).await?,
            Prepared::PostgresUnnamed => {
                return self
                    .execute_unnamed(sql, values, options.postgres_stream.as_ref())
                    .await;
            }
            Prepared::Emulated => {
                if !values.is_empty() {
                    sql.check_emulation()?;
                }
                let backslash_strings = !values.is_empty()
                    && (sql.has_sensitive_quotes()
                        || values
                            .iter()
                            .any(|value| matches!(value, BoundValue::String(_))))
                    && self.backslash_strings().await?;
                if !values.is_empty() && sql.has_sensitive_quotes() {
                    sql.check_string_mode(Syntax::Postgres { backslash_strings })?;
                }
                let mut literals = Vec::new();
                for value in values {
                    literals.push(match value {
                        BoundValue::String(bytes) => quote_string(bytes, backslash_strings),
                        value => literal(value, false),
                    });
                }
                let rendered = if values.is_empty() {
                    sql.unbound_sql().into_owned()
                } else {
                    sql.render(|i| literals[i].clone())
                };
                // Emulation sends SQL directly; PostgreSQL diagnoses unbound $n tokens.
                if sql.has_dollar_parameters {
                    return self.run_unprepared(&rendered).await;
                }
                self.prepare_query(&rendered).await?
            }
            _ => return Err(connection_error("Mismatched database driver")),
        };
        if let Prepared::Postgres { number, .. } | Prepared::PostgresDeferred { number } = prepared
            && values.len() != statement.params().len()
        {
            return Err(
                parameters::under_bound(&self.client, sql, &statement, values, *number).await,
            );
        }
        let parameters: Vec<TextParameter> = if matches!(prepared, Prepared::Emulated) {
            Vec::new()
        } else {
            values.iter().map(TextParameter::from).collect()
        };
        let references: Vec<&(dyn ToSql + Sync)> = parameters
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect();
        let query = sql.render(|i| format!("${}", i + 1));
        if let Some(owner) = &options.postgres_stream
            && !statement.columns().is_empty()
        {
            let stream = self
                .client
                .query_raw(&statement, references.iter().copied())
                .await
                .map_err(|failure| error(failure, &query))?;
            let columns = statement
                .columns()
                .iter()
                .map(crate::metadata::postgres)
                .collect();
            return self.start_lazy(stream, columns, query, owner).await;
        }
        let columns = statement
            .columns()
            .iter()
            .map(crate::metadata::postgres)
            .collect();
        let (values, affected) = if statement.columns().is_empty() {
            let affected = self
                .client
                .execute(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
            (Vec::new(), affected)
        } else {
            let rows = self
                .client
                .query(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
            let affected = rows.len() as u64;
            (text::convert(&self.client, rows, &query).await?, affected)
        };
        Ok(RawRows::Postgres {
            columns,
            rows: values,
            affected,
        })
    }

    pub async fn run_unprepared(&self, sql: &str) -> Result<RawRows, DbError> {
        let messages = self
            .client
            .simple_query(sql)
            .await
            .map_err(|e| error(e, sql))?;
        let mut affected = 0;
        let mut returns_rows = false;
        for message in messages {
            match message {
                SimpleQueryMessage::RowDescription(_) => returns_rows = true,
                SimpleQueryMessage::CommandComplete(count) => {
                    affected = if returns_rows { 0 } else { count };
                    returns_rows = false;
                }
                _ => {}
            }
        }
        Ok(RawRows::Affected(affected))
    }

    pub async fn quote(&self, bytes: &[u8]) -> Result<String, DbError> {
        Ok(quote_string(bytes, self.backslash_strings().await?))
    }

    pub async fn backslash_strings(&self) -> Result<bool, DbError> {
        let rows = self
            .client
            .simple_query("SHOW standard_conforming_strings")
            .await
            .map_err(|e| error(e, ""))?;
        Ok(rows.iter().any(|row| {
            matches!(row,
            SimpleQueryMessage::Row(row) if row.get(0) == Some("off"))
        }))
    }

    pub async fn last_id(&self, name: Option<&str>) -> Result<String, DbError> {
        let row = match name {
            Some(name) => {
                self.client
                    .query_one("SELECT currval($1::text)", &[&name])
                    .await
            }
            None => self.client.query_one("SELECT lastval()", &[]).await,
        }
        .map_err(|e| error(e, ""))?;
        let id: i64 = row.try_get(0).map_err(|e| error(e, ""))?;
        Ok(id.to_string())
    }
}

fn quote_string(bytes: &[u8], backslash_strings: bool) -> String {
    let quoted = quote(bytes, false);
    if backslash_strings {
        quoted.replace('\\', "\\\\")
    } else {
        quoted
    }
}
