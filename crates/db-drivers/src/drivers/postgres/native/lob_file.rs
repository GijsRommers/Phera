//! `pg_lo_import($db, $path)` sends local bytes over the existing session.

use std::path::PathBuf;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_postgres::types::Type;

use super::{Postgres, error};
use crate::errors::DbError;

const CHUNK: i32 = 64 * 1024;
const READ: i32 = 0x40000;
const WRITE: i32 = 0x20000;

impl Postgres {
    pub async fn import_lob(&self, path: PathBuf, requested: u32) -> Result<u32, DbError> {
        let mut file = File::open(path).await.map_err(file_error)?;
        let create = "SELECT pg_catalog.lo_create($1)";
        let oid: u32 = self
            .client
            .query_typed_one(create, &[(&requested, Type::OID)])
            .await
            .map_err(|cause| error(cause, create))?
            .try_get(0)
            .map_err(|cause| error(cause, create))?;
        let descriptor = self.open_lob(oid, WRITE).await?;
        let result = self.write_lob_chunks(descriptor, &mut file).await;
        let _ = self.close_lob(descriptor).await;
        result.map(|()| oid)
    }

    async fn write_lob_chunks(&self, descriptor: i32, file: &mut File) -> Result<(), DbError> {
        let mut bytes = vec![0; CHUNK as usize];
        loop {
            let length = file.read(&mut bytes).await.map_err(file_error)?;
            if length == 0 {
                return Ok(());
            }
            let chunk = &bytes[..length];
            let write = "SELECT pg_catalog.lowrite($1, $2)";
            self.client
                .query_typed_one(write, &[(&descriptor, Type::INT4), (&chunk, Type::BYTEA)])
                .await
                .map_err(|cause| error(cause, write))?;
        }
    }

    pub async fn export_lob(&self, path: PathBuf, oid: u32) -> Result<(), DbError> {
        let descriptor = self.open_lob(oid, READ).await?;
        let result = self.read_lob_chunks(descriptor, path).await;
        let _ = self.close_lob(descriptor).await;
        result
    }

    async fn read_lob_chunks(&self, descriptor: i32, path: PathBuf) -> Result<(), DbError> {
        let mut file = File::create(path).await.map_err(file_error)?;
        let read = "SELECT pg_catalog.loread($1, $2)";
        loop {
            let row = self
                .client
                .query_typed_one(read, &[(&descriptor, Type::INT4), (&CHUNK, Type::INT4)])
                .await
                .map_err(|cause| error(cause, read))?;
            let chunk: &[u8] = row.try_get(0).map_err(|cause| error(cause, read))?;
            file.write_all(chunk).await.map_err(file_error)?;
            if chunk.len() < CHUNK as usize {
                return file.flush().await.map_err(file_error);
            }
        }
    }

    async fn open_lob(&self, oid: u32, mode: i32) -> Result<i32, DbError> {
        let open = "SELECT pg_catalog.lo_open($1, $2)";
        self.client
            .query_typed_one(open, &[(&oid, Type::OID), (&mode, Type::INT4)])
            .await
            .map_err(|cause| error(cause, open))?
            .try_get(0)
            .map_err(|cause| error(cause, open))
    }

    async fn close_lob(&self, descriptor: i32) -> Result<(), DbError> {
        let close = "SELECT pg_catalog.lo_close($1)";
        self.client
            .query_typed_one(close, &[(&descriptor, Type::INT4)])
            .await
            .map(|_| ())
            .map_err(|cause| error(cause, close))
    }
}

fn file_error(cause: std::io::Error) -> DbError {
    DbError::general(cause.to_string())
}
