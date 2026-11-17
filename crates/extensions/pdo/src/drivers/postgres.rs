//! PostgreSQL prepares use `$1` on the wire: `$pdo->prepare('SELECT ?::int')`.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use super::postgres_protocol::TextParameter;
pub use super::postgres_protocol::error;
use super::postgres_stream::ObservedStream;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Config, NoTls, SimpleQueryMessage, Statement};

use super::{Column, Prepared, RawRows, connection_error};
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::Sql;

pub struct Postgres {
    client: Client,
    task: tokio::task::JoinHandle<()>,
    pub version: String,
    transaction: Arc<AtomicU8>,
    retire_client: OnceLock<()>,
}

impl Drop for Postgres {
    fn drop(&mut self) {
        // Closing the socket causes PostgreSQL to roll back any open transaction.
        self.task.abort();
    }
}

impl Postgres {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let mut config = Config::new();
        config
            .host(dsn.get("host", "localhost"))
            .port(dsn.port().map_err(|e| connection_error(format!("{e:?}")))?)
            .dbname(dsn.get("dbname", user))
            .user(user)
            .password(password)
            .ssl_mode(tokio_postgres::config::SslMode::Disable);
        if let Some(options) = dsn.fields.get("options") {
            config.options(options);
        }
        if let Some(name) = dsn.fields.get("application_name") {
            config.application_name(name);
        }
        let transaction = Arc::new(AtomicU8::new(b'I'));
        let stream = ObservedStream::connect(dsn, transaction.clone())
            .await
            .map_err(connection_error)?;
        let (client, connection) = config.connect_raw(stream, NoTls).await.map_err(|error| {
            let detail = match error.as_db_error() {
                Some(database) => format!("{}:  {}", database.severity(), database.message()),
                None => error.to_string(),
            };
            connection_error(format!(
                "connection to server at \"{}\", port {} failed: {detail}",
                dsn.get("host", "localhost"),
                dsn.get("port", "5432")
            ))
        })?;
        let task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut driver = Self {
            client,
            task,
            version: String::new(),
            transaction,
            retire_client: OnceLock::new(),
        };
        let messages = driver
            .client
            .simple_query("SHOW server_version")
            .await
            .map_err(|e| error(e, "SHOW server_version"))?;
        for message in messages {
            if let SimpleQueryMessage::Row(row) = message {
                driver.version = row.get(0).unwrap_or_default().into();
            }
        }
        Ok(driver)
    }

    pub async fn transaction_state(&self) -> bool {
        // An error may be delivered before the connection task consumes its
        // ReadyForQuery. An empty query is valid even in a failed transaction
        // and acts as a protocol barrier when the PHP script asks for state.
        let _ = self.client.simple_query("").await;
        !self.client.is_closed() && self.transaction.load(Ordering::Acquire) != b'I'
    }

    pub async fn metadata(&self, column: &Column) -> Result<(i16, Option<String>), DbError> {
        super::postgres_metadata::get(&self.client, column).await
    }

    pub async fn close(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }

    pub async fn reset(&self) -> Result<(), DbError> {
        // Separate protocol messages: DISCARD ALL cannot execute in a transaction.
        self.client
            .batch_execute("ROLLBACK")
            .await
            .map_err(|e| error(e, "ROLLBACK"))?;
        self.client
            .batch_execute("DISCARD ALL")
            .await
            .map_err(|e| error(e, "DISCARD ALL"))
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

    pub async fn prepare(&self, sql: &Sql) -> Result<Statement, DbError> {
        let query = sql.render(|i| format!("${}", i + 1));
        self.prepare_query(&query).await
    }

    async fn prepare_query(&self, query: &str) -> Result<Statement, DbError> {
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
            let _ = self.retire_client.set(());
        }
        result
    }

    pub fn reusable(&self) -> bool {
        self.retire_client.get().is_none()
    }

    pub async fn execute(
        &self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
    ) -> Result<RawRows, DbError> {
        let statement = match prepared {
            Prepared::Postgres(statement, _) => statement.clone(),
            Prepared::PostgresDeferred(_) => self.prepare(sql).await?,
            Prepared::Emulated => {
                let mut literals = Vec::new();
                for value in values {
                    literals.push(match value {
                        BoundValue::String(bytes) => self.quote(bytes).await?,
                        value => literal(value, false),
                    });
                }
                let rendered = if values.is_empty() {
                    sql.source.clone()
                } else {
                    sql.render(|i| literals[i].clone())
                };
                self.prepare_query(&rendered).await?
            }
            _ => return Err(connection_error("Mismatched database driver")),
        };
        if let Prepared::Postgres(_, number) | Prepared::PostgresDeferred(number) = prepared
            && values.len() != statement.params().len()
        {
            return Err(super::postgres_parameters::under_bound(
                &self.client,
                sql,
                &statement,
                values,
                *number,
            )
            .await);
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
        let columns = statement
            .columns()
            .iter()
            .map(crate::metadata::postgres)
            .collect();
        let mut values = Vec::new();
        let affected;
        if statement.columns().is_empty() {
            affected = self
                .client
                .execute(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
        } else {
            let rows = self
                .client
                .query(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
            affected = rows.len() as u64;
            for row in rows {
                let formatted = super::postgres_text::convert(&self.client, &row, &query).await?;
                values.push((row, formatted));
            }
        }
        Ok(RawRows::Postgres {
            columns,
            rows: values,
            affected,
        })
    }

    pub async fn direct(&self, sql: &str) -> Result<RawRows, DbError> {
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
        let rows = self
            .client
            .simple_query("SHOW standard_conforming_strings")
            .await
            .map_err(|e| error(e, ""))?;
        let escape = rows.iter().any(|row| {
            matches!(row,
            SimpleQueryMessage::Row(row) if row.get(0) == Some("off"))
        });
        let quoted = quote(bytes, false);
        Ok(if escape {
            quoted.replace('\\', "\\\\")
        } else {
            quoted
        })
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
