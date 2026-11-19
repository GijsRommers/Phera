//! PostgreSQL COPY shares the connection and authentication used by `pg_query()`.

use bytes::Bytes;
use futures_util::{SinkExt, TryStreamExt, pin_mut};

use super::{Postgres, error};
use crate::errors::DbError;

impl Postgres {
    pub async fn native_copy_from(&self, sql: &str, rows: Vec<Vec<u8>>) -> Result<u64, DbError> {
        let copy = self
            .client
            .copy_in::<_, Bytes>(sql)
            .await
            .map_err(|cause| error(cause, sql))?;
        pin_mut!(copy);
        for row in rows {
            copy.send(Bytes::from(row))
                .await
                .map_err(|cause| error(cause, sql))?;
        }
        copy.finish().await.map_err(|cause| error(cause, sql))
    }

    pub async fn native_copy_to(&self, sql: &str) -> Result<Vec<Vec<u8>>, DbError> {
        let copy = self
            .client
            .copy_out(sql)
            .await
            .map_err(|cause| error(cause, sql))?;
        copy.map_ok(|row| row.to_vec())
            .try_collect()
            .await
            .map_err(|cause| error(cause, sql))
    }
}
