//! PostgreSQL COPY uses the connection and authentication of `pg_query()`.

use postgres_protocol::message::{backend::Message, frontend};

use super::Postgres;
use crate::drivers::postgres::wire::MAX_MESSAGE;
use crate::errors::DbError;

impl Postgres {
    pub async fn native_copy_from(
        &mut self,
        sql: &str,
        rows: Vec<Vec<u8>>,
    ) -> Result<u64, DbError> {
        self.begin_copy(sql).await?;
        for row in rows {
            self.put_line(row).await?;
        }
        self.finish_copy().await
    }

    pub async fn native_copy_to(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, DbError> {
        if self.native()?.copy {
            return Err(DbError::general("COPY is in progress"));
        }
        self.client.simple(sql).await?;
        match self.client.next().await? {
            Message::CopyOutResponse(_) => {}
            Message::ErrorResponse(error) => {
                return Err(self.client.failure(error, sql, false).await);
            }
            _ => return Err(self.client.invalid("PostgreSQL did not start COPY OUT")),
        }
        let mut rows = Vec::new();
        loop {
            match self.client.next().await? {
                Message::CopyData(row) => rows.push(row.data().to_vec()),
                Message::CopyDone => {
                    self.finish(sql).await?;
                    return Ok(rows);
                }
                Message::ErrorResponse(error) => {
                    return Err(self.client.failure(error, sql, false).await);
                }
                _ => {
                    return Err(self
                        .client
                        .invalid("unexpected PostgreSQL COPY OUT response"));
                }
            }
        }
    }

    pub(super) async fn begin_copy(&mut self, sql: &str) -> Result<(), DbError> {
        if self.native()?.copy {
            return Err(DbError::general("COPY is in progress"));
        }
        self.client.simple(sql).await?;
        match self.client.next().await? {
            Message::CopyInResponse(_) => {
                self.native_mut()?.copy = true;
                Ok(())
            }
            Message::ErrorResponse(error) => Err(self.client.failure(error, sql, false).await),
            _ => Err(self.client.invalid("PostgreSQL did not start COPY IN")),
        }
    }

    pub async fn put_line(&mut self, row: Vec<u8>) -> Result<(), DbError> {
        if !self.native()?.copy {
            return Err(DbError::general("no COPY in progress"));
        }
        if matches!(row.as_slice(), b"\\.\n" | b"\\.\r\n") {
            return Ok(());
        }
        if row.len() > MAX_MESSAGE - 4 {
            return Err(self
                .client
                .invalid("PostgreSQL COPY row exceeds the 64 MiB limit"));
        }
        self.client
            .send(|out| {
                frontend::CopyData::new(row.as_slice())?.write(out);
                Ok(())
            })
            .await
    }

    pub async fn end_copy(&mut self) -> Result<(), DbError> {
        self.finish_copy().await.map(|_| ())
    }

    async fn finish_copy(&mut self) -> Result<u64, DbError> {
        if !self.native()?.copy {
            return Err(DbError::general("no COPY in progress"));
        }
        self.client
            .send(|out| {
                frontend::copy_done(out);
                Ok(())
            })
            .await?;
        self.native_mut()?.copy = false;
        self.finish("").await
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
