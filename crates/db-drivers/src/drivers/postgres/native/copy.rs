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

pub(super) fn is_copy_from_stdin(sql: &str) -> bool {
    let mut words = sql
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ';')
        .filter(|word| !word.is_empty());
    words
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("copy"))
        && words.any(|word| word.eq_ignore_ascii_case("from"))
        && words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("stdin"))
}

impl Postgres {
    pub(super) async fn begin_copy(&mut self, sql: &str) -> Result<(), DbError> {
        let sink = self
            .client
            .copy_in(sql)
            .await
            .map_err(|cause| error(cause, sql))?;
        self.native_mut()?.copy = Some(Box::pin(sink));
        Ok(())
    }

    pub async fn put_line(&mut self, row: Vec<u8>) -> Result<(), DbError> {
        let copy = self
            .native_mut()?
            .copy
            .as_mut()
            .ok_or_else(|| DbError::general("no COPY in progress"))?;
        if matches!(row.as_slice(), b"\\.\n" | b"\\.\r\n") {
            return Ok(());
        }
        copy.send(Bytes::from(row))
            .await
            .map_err(|cause| error(cause, ""))
    }

    pub async fn end_copy(&mut self) -> Result<(), DbError> {
        let mut copy = self
            .native_mut()?
            .copy
            .take()
            .ok_or_else(|| DbError::general("no COPY in progress"))?;
        copy.as_mut()
            .finish()
            .await
            .map_err(|cause| error(cause, ""))
            .map(|_| ())
    }
}
