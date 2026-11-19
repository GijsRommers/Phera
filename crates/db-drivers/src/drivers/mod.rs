//! The database drivers behind PDO. Every PDO method reaches its database
//! through [`Driver`]; nothing outside this module knows a client library.
//!
//! ```php
//! $pdo = new PDO('pgsql:host=127.0.0.1;dbname=app', $user, $password); // Driver::connect
//! $statement = $pdo->prepare('SELECT * FROM t WHERE id = ?');           // Driver::prepare
//! $statement->execute([42]);                                           // Driver::execute
//! $statement->fetchAll();                                              // RawRows::into_php
//! ```
//!
//! A driver's methods run on the I/O runtime (see `crate::pool`), so what
//! goes in and comes out is plain data that may cross threads: SQL, bound
//! values, and [`RawRows`], which the PHP worker turns into PHP values.
//!
//! # Adding a driver
//!
//! 1. Write a module here whose connection type has the methods that the
//!    PostgreSQL and MySQL arms below call, add it as a variant of
//!    [`Driver`], and give every method its arm; the compiler points out
//!    each one that is missing.
//! 2. Add its [`DriverName`] and DSN prefix in `crate::dsn`.
//! 3. Add its prepared statement to [`Prepared`], its result to
//!    [`RawRows`] and its column description to `crate::metadata::Metadata`.
//! 4. Compare it with PHP: scripts in `tests/scripts`.

pub mod mysql;
pub mod postgres;
pub mod sqlite;
mod tls;

use std::sync::Arc;
use std::time::Duration;

use phera_extension_api::Value;

use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

/// One column of a result.
#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    /// What `getColumnMeta()` reports.
    pub metadata: Metadata,
}

/// A result as PHP values, kept by the statement for its fetch calls.
#[derive(Default)]
pub struct Rows {
    pub columns: Vec<Column>,
    pub values: Vec<Vec<Value>>,
    /// `rowCount()`: rows changed, or for a query the rows returned.
    pub affected: u64,
}

#[derive(Default)]
pub struct Execution {
    pub national_strings: bool,
    pub mysql_stream: Option<Arc<()>>,
    pub postgres_stream: Option<Arc<()>>,
}

/// How a PDO statement is executed.
#[derive(Clone)]
pub enum Prepared {
    /// `PDO::ATTR_EMULATE_PREPARES`: the values are quoted into the SQL.
    Emulated,
    /// A PostgreSQL statement that is prepared on its first execute, as
    /// pdo_pgsql does. `number` names it, as `pdo_stmt_0000000a`, in errors.
    PostgresDeferred {
        number: u64,
    },
    Postgres {
        statement: tokio_postgres::Statement,
        number: u64,
    },
    /// Native binds to an unnamed statement, `Pdo\Pgsql::ATTR_DISABLE_PREPARES`.
    PostgresUnnamed,
    Mysql(mysql_async::Statement),
    /// A live SQLite statement on its database's thread; see `sqlite`.
    Sqlite(phera_sqlite::Statement),
}

/// A result as the driver returned it. It becomes PHP values on the worker,
/// in [`RawRows::into_php`], because PHP values cannot cross threads.
pub enum RawRows {
    /// No result set, only the number of rows changed.
    Affected(u64),
    Postgres {
        columns: Vec<Column>,
        rows: Vec<postgres::Row>,
        affected: u64,
    },
    Mysql {
        columns: Vec<mysql_async::Column>,
        rows: Vec<mysql_async::Row>,
        affected: u64,
    },
}

impl RawRows {
    pub fn has_columns(&self) -> bool {
        match self {
            Self::Affected(_) => false,
            Self::Postgres { columns, .. } => !columns.is_empty(),
            Self::Mysql { columns, .. } => !columns.is_empty(),
        }
    }

    pub fn into_php(self) -> Result<Rows, DbError> {
        match self {
            Self::Affected(affected) => Ok(Rows {
                affected,
                ..Rows::default()
            }),
            Self::Postgres {
                columns,
                rows,
                affected,
            } => postgres::values::convert(columns, rows, affected),
            Self::Mysql {
                columns,
                rows,
                affected,
            } => Ok(mysql::values::convert(columns, rows, affected)),
        }
    }
}

/// An open connection to one of the databases PDO supports.
pub enum Driver {
    Postgres(postgres::Postgres),
    Mysql(mysql::Mysql),
    /// Never pooled, and its statements run outside this interface; see
    /// the `sqlite` module for why.
    Sqlite(sqlite::Sqlite),
}

impl Driver {
    /// `new PDO($dsn, $username, $password)`.
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        match dsn.driver {
            DriverName::Postgres => postgres::Postgres::connect(dsn, user, password)
                .await
                .map(Self::Postgres),
            DriverName::Mysql => mysql::Mysql::connect(dsn, user, password)
                .await
                .map(Self::Mysql),
            DriverName::Sqlite => sqlite::Sqlite::connect(dsn).await.map(Self::Sqlite),
        }
    }

    /// `PDO::ATTR_SERVER_VERSION`, read once while connecting.
    pub fn version(&self) -> &str {
        match self {
            Self::Postgres(driver) => &driver.version,
            Self::Mysql(driver) => &driver.version,
            Self::Sqlite(_) => phera_sqlite::version(),
        }
    }

    /// `$pdo->prepare($sql)` without emulation. PostgreSQL defers this to
    /// the first execute (see [`Prepared::PostgresDeferred`]), which then
    /// calls it with the statement's number.
    pub async fn prepare(&mut self, sql: &Sql, number: u64) -> Result<Prepared, DbError> {
        match self {
            Self::Postgres(driver) => driver
                .prepare(sql)
                .await
                .map(|statement| Prepared::Postgres { statement, number }),
            Self::Mysql(driver) => driver.prepare(sql).await.map(Prepared::Mysql),
            Self::Sqlite(driver) => driver
                .database
                .prepare(sql.source.clone())
                .await
                .map(Prepared::Sqlite)
                .map_err(sqlite::error),
        }
    }

    /// `$statement->execute()`: runs a prepared or emulated statement with
    /// its values, in the order of the placeholders.
    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: &Execution,
    ) -> Result<RawRows, DbError> {
        // Quoting a value into SQL text needs the value to be text.
        let binary = |value: &BoundValue| matches!(value, BoundValue::String(bytes) if std::str::from_utf8(bytes).is_err());
        if matches!(prepared, Prepared::Emulated) && values.iter().any(binary) {
            return Err(DbError::general(
                "Emulated non-UTF-8 parameters are not supported yet",
            ));
        }
        match self {
            Self::Postgres(driver) => driver.execute(sql, prepared, values, options).await,
            Self::Mysql(driver) => driver.execute(sql, prepared, values, options).await,
            Self::Sqlite(_) => Err(connection_error(
                "a SQLite statement is executed through its own handle",
            )),
        }
    }

    /// `$pdo->exec($sql)`: SQL text without placeholders, for its row count.
    pub async fn exec(&mut self, sql: &str) -> Result<RawRows, DbError> {
        match self {
            Self::Postgres(driver) => {
                driver.discard_lazy();
                driver.run_unprepared(sql).await
            }
            Self::Mysql(driver) => driver.exec(sql).await,
            Self::Sqlite(driver) => driver.run_unprepared(sql).await,
        }
    }

    /// SQL text sent as it is: `BEGIN`, `COMMIT` and `ROLLBACK`.
    pub async fn run_unprepared(&mut self, sql: &str) -> Result<RawRows, DbError> {
        match self {
            Self::Postgres(driver) => {
                driver.discard_lazy();
                driver.run_unprepared(sql).await
            }
            Self::Mysql(driver) => driver.run_unprepared(sql).await,
            Self::Sqlite(driver) => driver.run_unprepared(sql).await,
        }
    }

    /// `$pdo->lastInsertId($name)`.
    pub async fn last_id(&mut self, name: Option<&str>) -> Result<String, DbError> {
        match self {
            Self::Postgres(driver) => {
                driver.discard_lazy();
                driver.last_id(name).await
            }
            Self::Mysql(driver) => Ok(driver.last_id()),
            Self::Sqlite(driver) => driver
                .database
                .last_id()
                .await
                .map(|id| id.to_string())
                .map_err(sqlite::error),
        }
    }

    /// `$pdo->quote($string)`, which depends on the session's escaping mode.
    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        match self {
            Self::Postgres(driver) => {
                driver.buffer_lazy().await;
                driver.quote(bytes).await
            }
            Self::Mysql(driver) => driver.quote(bytes).await,
            Self::Sqlite(_) => Ok(String::from_utf8_lossy(&sqlite::quote(bytes)).into_owned()),
        }
    }

    /// What `getColumnMeta()` must ask the server: PostgreSQL's type length
    /// and table name. MySQL sends everything with the result.
    pub async fn postgres_column(
        &mut self,
        column: &Column,
    ) -> Result<(i16, Option<String>), DbError> {
        match self {
            Self::Postgres(driver) => {
                driver.discard_lazy();
                driver.metadata(column).await
            }
            Self::Mysql(_) | Self::Sqlite(_) => Err(connection_error("Mismatched metadata driver")),
        }
    }

    /// `$pdo->inTransaction()`, asked of the server.
    pub async fn transaction_state(&mut self) -> Option<bool> {
        match self {
            Self::Postgres(driver) => Some(driver.transaction_state().await),
            Self::Mysql(driver) => driver.transaction_state(),
            Self::Sqlite(driver) => driver.database.transaction().await.ok(),
        }
    }

    /// The transaction state the server reported with its last reply, for
    /// drivers whose replies carry it (MySQL).
    pub fn observed_transaction_state(&self) -> Option<bool> {
        match self {
            Self::Postgres(_) | Self::Sqlite(_) => None,
            Self::Mysql(driver) => driver.transaction_state(),
        }
    }

    /// Clears what a PDO object left behind before the pool lends the
    /// connection again.
    pub async fn reset(&mut self) -> Result<(), DbError> {
        match self {
            Self::Postgres(driver) => driver.reset().await,
            Self::Mysql(driver) => driver.reset().await,
            Self::Sqlite(_) => Err(connection_error("SQLite handles are never pooled")),
        }
    }

    pub async fn close(self) {
        match self {
            Self::Postgres(driver) => driver.close().await,
            Self::Mysql(driver) => driver.close().await,
            // Dropping the last handle ends the database's thread.
            Self::Sqlite(driver) => drop(driver),
        }
    }

    fn reusable(&self) -> bool {
        match self {
            Self::Postgres(driver) => driver.reusable(),
            Self::Mysql(_) => true,
            Self::Sqlite(_) => false,
        }
    }

    fn is_broken(&self) -> bool {
        match self {
            Self::Postgres(driver) => driver.is_broken(),
            Self::Mysql(driver) => driver.is_broken(),
            Self::Sqlite(_) => false,
        }
    }

    /// A round trip that shows the server still answers.
    async fn health(&mut self) -> Result<(), DbError> {
        match self {
            Self::Postgres(driver) => driver.health().await,
            Self::Mysql(driver) => driver.health().await,
            Self::Sqlite(driver) => driver.run_unprepared("SELECT 1").await.map(|_| ()),
        }
    }
}

/// An idle connection unused for this long is checked with a round trip
/// before it is lent again; a younger one is trusted.
const CHECK_AFTER: Duration = Duration::from_secs(3);

/// What the pool needs from a database connection.
impl phera_io_pool::Connection for Driver {
    type Error = DbError;

    async fn check(&mut self, idle: Duration) -> bool {
        !self.is_broken() && (idle < CHECK_AFTER || self.health().await.is_ok())
    }

    fn reusable(&self) -> bool {
        Driver::reusable(self)
    }

    async fn reset(&mut self) -> Result<(), DbError> {
        Driver::reset(self).await
    }

    async fn close(self) {
        Driver::close(self).await;
    }
}

/// `SQLSTATE[08006] [7] ...`, PHP's code for a connection that failed.
pub fn connection_error(message: impl ToString) -> DbError {
    DbError {
        state: "08006".into(),
        code: Some(7),
        detail: message.to_string(),
    }
}
