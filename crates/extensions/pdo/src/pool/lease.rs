//! A PDO borrows its worker's connection for each operation, including `$pdo->query($sql)`.

use std::cell::Cell;
use std::sync::Arc;

use phera_extension_api::PhpArray;

use crate::constants::ATTR_SERVER_INFO;
use crate::drivers::{Column, Driver, Execution, Prepared, Rows, connection_error};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

pub struct Lease {
    connection: phera_db_drivers::pool::Lease,
    version: String,
    observed_transaction: Cell<Option<bool>>,
}

impl Lease {
    pub(super) fn new(connection: phera_db_drivers::pool::Lease) -> Result<Self, DbError> {
        let (version, observed_transaction) = connection
            .try_inspect(|driver| {
                (
                    driver.version().to_owned(),
                    driver.observed_transaction_state(),
                )
            })
            .ok_or_else(|| connection_error("Missing server connection"))?;
        Ok(Self {
            connection,
            version,
            observed_transaction: Cell::new(observed_transaction),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    async fn run<T>(
        &self,
        operation: impl AsyncFnOnce(&mut Driver) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        self.connection
            .run(async move |driver| {
                let result = operation(driver).await;
                self.observed_transaction
                    .set(driver.observed_transaction_state());
                result
            })
            .await
    }

    pub async fn prepare(&self, sql: &Sql, number: u64) -> Result<Prepared, DbError> {
        self.run(async |driver| driver.prepare(sql, number).await)
            .await
    }

    pub async fn execute(
        &self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: Execution,
    ) -> Result<Rows, DbError> {
        self.run(async |driver| driver.execute(sql, prepared, values, &options).await)
            .await
    }

    pub async fn read_stream(
        &self,
        owner: Arc<()>,
        all: bool,
    ) -> Result<Option<(Rows, bool)>, DbError> {
        let result = self
            .run(async |driver| match driver {
                Driver::Mysql(mysql) => mysql.read_stream(&owner, all).await,
                Driver::Postgres(postgres) => postgres.read_lazy(&owner, all).await,
                Driver::Sqlite(_) => Ok(None),
            })
            .await?;
        result
            .map(|(rows, done)| rows.into_php().map(|rows| (rows, done)))
            .transpose()
    }

    pub async fn close_stream(&self, owner: Arc<()>) -> Result<(), DbError> {
        self.run(async |driver| {
            match driver {
                Driver::Mysql(mysql) => mysql.close_stream(&owner).await,
                Driver::Postgres(postgres) => postgres.close_lazy(&owner),
                Driver::Sqlite(_) => {}
            }
            Ok(())
        })
        .await
    }

    pub async fn exec(&self, sql: &str) -> Result<Rows, DbError> {
        self.run(async |driver| driver.exec(sql).await).await
    }

    pub async fn run_unprepared(&self, sql: &str) -> Result<Rows, DbError> {
        self.run(async |driver| driver.run_unprepared(sql).await)
            .await
    }

    pub async fn last_id(&self, name: Option<&str>) -> Result<String, DbError> {
        self.run(async |driver| driver.last_id(name).await).await
    }

    pub async fn quote(&self, bytes: &[u8]) -> Result<String, DbError> {
        self.run(async |driver| driver.quote(bytes).await).await
    }

    /// `getColumnMeta()`. Only PostgreSQL needs to ask the server.
    pub async fn metadata(&self, column: &Column) -> Result<PhpArray, DbError> {
        let postgres = match &column.metadata {
            Metadata::Mysql(mysql) => return Ok(crate::metadata::mysql(mysql)),
            Metadata::Sqlite(sqlite, kind) => {
                return Ok(crate::drivers::sqlite::statement::metadata(sqlite, *kind));
            }
            Metadata::Postgres(postgres) => postgres,
        };
        let (length, table) = self
            .run(async |driver| driver.postgres_column(column).await)
            .await?;
        Ok(crate::metadata::postgres_values(
            &column.name,
            postgres,
            length,
            table,
        ))
    }

    /// `ATTR_TIMEOUT` on a SQLite connection.
    pub async fn sqlite_timeout(&self, milliseconds: u64) -> Result<(), DbError> {
        self.run(async |driver| match driver {
            Driver::Sqlite(sqlite) => sqlite.set_timeout(milliseconds).await,
            _ => Err(connection_error("ATTR_TIMEOUT is set on SQLite only")),
        })
        .await
    }

    pub async fn mysql_autocommit(&self, enabled: bool) -> Result<(), DbError> {
        self.run(async |driver| match driver {
            Driver::Mysql(mysql) => mysql.set_autocommit(enabled).await,
            _ => Err(connection_error("ATTR_AUTOCOMMIT is set on MySQL only")),
        })
        .await
    }

    pub async fn server_attribute(&self, attribute: i64) -> Result<String, DbError> {
        self.run(async |driver| match (driver, attribute) {
            (Driver::Mysql(mysql), ATTR_SERVER_INFO) => mysql.server_info().await,
            (Driver::Postgres(postgres), ATTR_SERVER_INFO) => {
                postgres.buffer_lazy().await;
                postgres.server_info().await
            }
            (Driver::Mysql(mysql), _) => Ok(mysql.connection_status()),
            (Driver::Postgres(postgres), _) => Ok(postgres.connection_status().into()),
            (Driver::Sqlite(_), _) => Err(connection_error("Missing server connection")),
        })
        .await
    }

    pub fn observed_transaction_state(&self) -> Option<bool> {
        self.observed_transaction.get()
    }

    pub async fn transaction_state(&self) -> Option<bool> {
        self.run(async |driver| Ok(driver.transaction_state().await))
            .await
            .ok()
            .flatten()
    }

    #[cfg(test)]
    pub async fn reset(&self) -> Result<(), DbError> {
        self.run(async |driver| driver.reset().await).await
    }
}
