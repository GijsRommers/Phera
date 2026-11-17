//! Drivers share one interface: `$pdo->prepare($sql)->execute($values)` always awaits I/O.

pub mod mysql;
mod mysql_values;
pub mod sqlite;
mod transport;
pub use transport::RawRows;
pub mod postgres;
mod postgres_metadata;
mod postgres_parameters;
mod postgres_protocol;
mod postgres_stream;
mod postgres_text;
mod postgres_values;

use phera_extension_api::Value;

use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub metadata: crate::metadata::Metadata,
}

#[derive(Default)]
pub struct Rows {
    pub columns: Vec<Column>,
    pub values: Vec<Vec<Value>>,
    pub affected: u64,
}

#[derive(Clone)]
pub enum Prepared {
    Postgres(tokio_postgres::Statement, u64),
    Mysql(mysql_async::Statement),
    Emulated,
    PostgresDeferred(u64),
    Sqlite(phera_sqlite::Statement),
}

pub enum Driver {
    Postgres(postgres::Postgres),
    Mysql(mysql::Mysql),
    Sqlite(sqlite::Sqlite),
}

impl Driver {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        match dsn.driver {
            DriverName::Sqlite => sqlite::Sqlite::connect(dsn).await.map(Self::Sqlite),
            DriverName::Postgres => postgres::Postgres::connect(dsn, user, password)
                .await
                .map(Self::Postgres),
            DriverName::Mysql => mysql::Mysql::connect(dsn, user, password)
                .await
                .map(Self::Mysql),
        }
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Prepared, DbError> {
        match self {
            Self::Postgres(driver) => driver
                .prepare(sql)
                .await
                .map(|statement| Prepared::Postgres(statement, 0)),
            Self::Mysql(driver) => driver.prepare(sql).await.map(Prepared::Mysql),
            Self::Sqlite(driver) => driver
                .database
                .prepare(sql.source.clone())
                .await
                .map(Prepared::Sqlite)
                .map_err(sqlite::error),
        }
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
    ) -> Result<RawRows, DbError> {
        if matches!(prepared, Prepared::Emulated) && values.iter().any(|value| {
            matches!(value, BoundValue::String(bytes) if std::str::from_utf8(bytes).is_err())
        }) {
            return Err(DbError { state: "HY000".into(), code: None,
                detail: "Emulated non-UTF-8 parameters are not supported yet".into() });
        }
        match self {
            Self::Postgres(driver) => driver.execute(sql, prepared, values).await,
            Self::Mysql(driver) => driver.execute(sql, prepared, values).await,
            Self::Sqlite(_) => Err(connection_error("SQLite executes its native binding map")),
        }
    }

    pub async fn exec(&mut self, sql: &str) -> Result<RawRows, DbError> {
        match self {
            Self::Postgres(driver) => driver.direct(sql).await,
            Self::Mysql(driver) => driver.exec(sql).await,
            Self::Sqlite(driver) => driver
                .database
                .exec(sql.into())
                .await
                .map(RawRows::Affected)
                .map_err(sqlite::error),
        }
    }

    pub async fn direct(&mut self, sql: &str) -> Result<RawRows, DbError> {
        match self {
            Self::Postgres(driver) => driver.direct(sql).await,
            Self::Mysql(driver) => driver.direct(sql).await,
            Self::Sqlite(driver) => driver
                .database
                .exec(sql.into())
                .await
                .map(RawRows::Affected)
                .map_err(sqlite::error),
        }
    }

    pub async fn last_id(&mut self, name: Option<&str>) -> Result<String, DbError> {
        match self {
            Self::Postgres(driver) => driver.last_id(name).await,
            Self::Mysql(driver) => Ok(driver.last_id()),
            Self::Sqlite(driver) => driver
                .database
                .last_id()
                .await
                .map(|id| id.to_string())
                .map_err(sqlite::error),
        }
    }

    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        match self {
            Self::Postgres(driver) => driver.quote(bytes).await,
            Self::Mysql(driver) => driver.quote(bytes).await,
            Self::Sqlite(_) => Ok(crate::parameters::quote(
                bytes.split(|b| *b == 0).next().unwrap_or_default(),
                false,
            )),
        }
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        match self {
            Self::Postgres(driver) => driver.reset().await,
            Self::Mysql(driver) => driver.reset().await,
            Self::Sqlite(_) => Err(connection_error("SQLite handles are never pooled")),
        }
    }

    pub async fn metadata(
        &mut self,
        column: &Column,
    ) -> Result<Option<(i16, Option<String>)>, DbError> {
        match self {
            Self::Postgres(driver) => driver.metadata(column).await.map(Some),
            Self::Mysql(_) | Self::Sqlite(_) => Ok(None),
        }
    }

    pub async fn close(self) {
        match self {
            Self::Postgres(driver) => driver.close().await,
            Self::Mysql(driver) => driver.close().await,
            Self::Sqlite(driver) => drop(driver),
        }
    }

    pub async fn transaction_state(&mut self) -> Option<bool> {
        match self {
            Self::Mysql(driver) => driver.transaction_state(),
            Self::Postgres(driver) => Some(driver.transaction_state().await),
            Self::Sqlite(driver) => driver.database.transaction().await.ok(),
        }
    }

    pub fn observed_transaction_state(&self) -> Option<bool> {
        match self {
            Self::Mysql(driver) => driver.transaction_state(),
            Self::Postgres(_) | Self::Sqlite(_) => None,
        }
    }

    pub fn reusable(&self) -> bool {
        match self {
            Self::Postgres(driver) => driver.reusable(),
            Self::Mysql(_) => true,
            Self::Sqlite(_) => false,
        }
    }

    pub fn is_broken(&self) -> bool {
        match self {
            Self::Postgres(driver) => driver.is_broken(),
            Self::Mysql(driver) => driver.is_broken(),
            Self::Sqlite(_) => false,
        }
    }

    pub async fn health(&mut self) -> Result<(), DbError> {
        match self {
            Self::Postgres(driver) => driver.health().await,
            Self::Mysql(driver) => driver.health().await,
            Self::Sqlite(driver) => driver
                .database
                .exec("SELECT 1".into())
                .await
                .map(|_| ())
                .map_err(sqlite::error),
        }
    }

    pub fn version(&self) -> &str {
        match self {
            Self::Postgres(driver) => &driver.version,
            Self::Mysql(driver) => &driver.version,
            Self::Sqlite(_) => phera_sqlite::version(),
        }
    }
}

pub fn connection_error(message: impl ToString) -> DbError {
    DbError {
        state: "08006".into(),
        code: Some(7),
        detail: message.to_string(),
    }
}
