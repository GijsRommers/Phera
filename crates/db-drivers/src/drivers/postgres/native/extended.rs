//! `pg_query_params` and `pg_execute` get PostgreSQL's text in their original Bind.

use super::{NativeResult, NativeSequence, Postgres};
use crate::errors::DbError;
use crate::parameters::BoundValue;

impl Postgres {
    pub async fn native_prepare(
        &mut self,
        name: &str,
        sql: &str,
    ) -> Result<NativeSequence, DbError> {
        let state = self.native()?;
        if state.copy {
            return Err(DbError::general("COPY is in progress"));
        }
        self.client.prepare(name, sql).await?;
        self.finish(sql).await?;
        Ok(vec![NativeResult {
            command: "PREPARE".into(),
            ..NativeResult::default()
        }])
    }

    pub async fn native_execute(
        &mut self,
        name: &str,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        self.native_bound(name, None, params).await
    }

    pub async fn native_query_params(
        &mut self,
        sql: &str,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        self.native_bound("", Some(sql), params).await
    }

    async fn native_bound(
        &mut self,
        name: &str,
        sql: Option<&str>,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        if self.native()?.copy {
            return Err(DbError::general("COPY is in progress"));
        }
        self.client.extended(name, sql, params, &[]).await?;
        self.collect_native(sql.unwrap_or(""), true).await
    }
}
