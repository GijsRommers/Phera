//! What a PDO object can do with its borrowed connection. Each method sends
//! owned data to the I/O runtime and converts the driver's answer back into
//! PHP values here, on the worker.

use phera_extension_api::PhpArray;
use phera_io_pool::Operation;

use crate::drivers::{Column, Driver, Prepared, Rows, connection_error};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

pub struct Lease {
    connection: phera_io_pool::Lease<Driver>,
    /// `PDO::ATTR_SERVER_VERSION`, read once at connect time.
    version: String,
    /// The transaction state the server reported after the last operation,
    /// for drivers that report it with every reply (MySQL).
    observed_transaction: Option<bool>,
}

impl Lease {
    pub(super) fn new(connection: phera_io_pool::Lease<Driver>) -> Self {
        let (version, observed_transaction) = connection
            .try_inspect(|driver| {
                (
                    driver.version().to_owned(),
                    driver.observed_transaction_state(),
                )
            })
            .expect("no operation uses a lease that was just lent");
        Self {
            connection,
            version,
            observed_transaction,
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Runs one driver operation on the I/O runtime, and remembers the
    /// transaction state the server reported with it.
    async fn run<T, F>(&mut self, operation: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut Driver) -> Operation<'a, Result<T, DbError>> + Send + 'static,
    {
        let (result, observed) = self
            .connection
            .run(|driver| {
                Box::pin(async move {
                    let result = operation(driver).await;
                    (result, driver.observed_transaction_state())
                })
            })
            .await
            .map_err(connection_error)?;
        self.observed_transaction = observed;
        result
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Prepared, DbError> {
        let sql = sql.clone();
        self.run(move |driver| Box::pin(async move { driver.prepare(&sql).await }))
            .await
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
    ) -> Result<Rows, DbError> {
        let (sql, prepared, values) = (sql.clone(), prepared.clone(), values.to_vec());
        self.run(move |driver| {
            Box::pin(async move { driver.execute(&sql, &prepared, &values).await })
        })
        .await?
        .into_php()
    }

    pub async fn exec(&mut self, sql: &str) -> Result<Rows, DbError> {
        let sql = sql.to_owned();
        self.run(move |driver| Box::pin(async move { driver.exec(&sql).await }))
            .await?
            .into_php()
    }

    pub async fn direct(&mut self, sql: &str) -> Result<Rows, DbError> {
        let sql = sql.to_owned();
        self.run(move |driver| Box::pin(async move { driver.direct(&sql).await }))
            .await?
            .into_php()
    }

    pub async fn last_id(&mut self, name: Option<&str>) -> Result<String, DbError> {
        let name = name.map(str::to_owned);
        self.run(move |driver| Box::pin(async move { driver.last_id(name.as_deref()).await }))
            .await
    }

    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        let bytes = bytes.to_vec();
        self.run(move |driver| Box::pin(async move { driver.quote(&bytes).await }))
            .await
    }

    pub async fn metadata(&mut self, column: &Column) -> Result<PhpArray, DbError> {
        let owned = column.clone();
        let result = self
            .run(move |driver| Box::pin(async move { driver.metadata(&owned).await }))
            .await?;
        match (&column.metadata, result) {
            (Metadata::Postgres { .. }, Some((length, table))) => {
                Ok(crate::metadata::postgres_values(column, length, table))
            }
            (Metadata::Mysql(column), None) => Ok(crate::metadata::mysql(column)),
            _ => Err(connection_error("Mismatched metadata driver")),
        }
    }

    pub fn observed_transaction_state(&self) -> Option<bool> {
        self.observed_transaction
    }

    pub async fn transaction_state(&mut self) -> Option<bool> {
        self.run(|driver| Box::pin(async move { Ok(driver.transaction_state().await) }))
            .await
            .ok()
            .flatten()
    }

    #[cfg(test)]
    pub async fn reset(&mut self) -> Result<(), DbError> {
        self.run(|driver| Box::pin(async move { driver.reset().await }))
            .await
    }
}
