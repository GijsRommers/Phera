//! Each submitted operation retains the lease, even if the PHP request is cancelled.

use std::future::Future;
use std::pin::Pin;

use phera_extension_api::PhpArray;

use super::{Lease, runtime};
use crate::drivers::{Column, Driver, Prepared, Rows, connection_error};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

type Operation<'a, T> = Pin<Box<dyn Future<Output = Result<T, DbError>> + Send + 'a>>;

impl Lease {
    async fn run<T, F>(&mut self, operation: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut Driver) -> Operation<'a, T> + Send + 'static,
    {
        let lease = self.shared.clone();
        let (result, observed) = runtime::run(async move {
            let mut guard = lease.driver.lock().await;
            let driver = guard.as_mut().expect("a live lease owns its driver");
            let result = operation(driver).await;
            Ok((result, driver.observed_transaction_state()))
        })
        .await?;
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
