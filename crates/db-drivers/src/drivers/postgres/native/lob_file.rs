//! `pg_lo_import` and `pg_lo_export` stream files over the existing session.

use std::path::PathBuf;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::Postgres;
use crate::drivers::connection_error;
use crate::errors::DbError;
use crate::parameters::BoundValue;

const CHUNK: usize = 64 * 1024;
const READ: i64 = 0x40000;
const WRITE: i64 = 0x20000;
const HEX: &[u8; 16] = b"0123456789abcdef";

impl Postgres {
    pub async fn import_lob(&mut self, path: PathBuf, requested: u32) -> Result<u32, DbError> {
        let mut file = File::open(path).await.map_err(file_error)?;
        let oid = self
            .lob_number("SELECT pg_catalog.lo_create($1)", &[i64::from(requested)])
            .await?;
        let descriptor = self.open_lob(i64::from(oid), WRITE).await?;
        let result = self.write_lob_chunks(descriptor, &mut file).await;
        let _ = self.close_lob(descriptor).await;
        result.map(|()| oid)
    }

    async fn write_lob_chunks(&mut self, descriptor: i64, file: &mut File) -> Result<(), DbError> {
        let mut bytes = vec![0; CHUNK];
        loop {
            let length = file.read(&mut bytes).await.map_err(file_error)?;
            if length == 0 {
                return Ok(());
            }
            let mut hex = Vec::with_capacity(length * 2);
            for byte in &bytes[..length] {
                hex.push(HEX[usize::from(*byte >> 4)]);
                hex.push(HEX[usize::from(*byte & 15)]);
            }
            self.single_row(
                "SELECT pg_catalog.lowrite($1, pg_catalog.decode($2, 'hex'))",
                &[BoundValue::Int(descriptor), BoundValue::String(hex)],
            )
            .await?;
        }
    }

    pub async fn export_lob(&mut self, path: PathBuf, oid: u32) -> Result<(), DbError> {
        let descriptor = self.open_lob(i64::from(oid), READ).await?;
        let result = self.read_lob_chunks(descriptor, path).await;
        let _ = self.close_lob(descriptor).await;
        result
    }

    async fn read_lob_chunks(&mut self, descriptor: i64, path: PathBuf) -> Result<(), DbError> {
        let mut file = File::create(path).await.map_err(file_error)?;
        loop {
            let row = self
                .single_row(
                    "SELECT pg_catalog.encode(pg_catalog.loread($1, $2), 'hex')",
                    &[BoundValue::Int(descriptor), BoundValue::Int(CHUNK as i64)],
                )
                .await?;
            let hex = first_cell(row)?;
            if hex.len() % 2 != 0 {
                return Err(connection_error("Invalid PostgreSQL large object data"));
            }
            let chunk = hex
                .chunks_exact(2)
                .map(|pair| {
                    let high = hex_digit(pair[0])?;
                    let low = hex_digit(pair[1])?;
                    Ok(high * 16 + low)
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            file.write_all(&chunk).await.map_err(file_error)?;
            if chunk.len() < CHUNK {
                return file.flush().await.map_err(file_error);
            }
        }
    }

    async fn open_lob(&mut self, oid: i64, mode: i64) -> Result<i64, DbError> {
        self.lob_number("SELECT pg_catalog.lo_open($1, $2)", &[oid, mode])
            .await
    }

    async fn close_lob(&mut self, descriptor: i64) -> Result<(), DbError> {
        self.single_row(
            "SELECT pg_catalog.lo_close($1)",
            &[BoundValue::Int(descriptor)],
        )
        .await
        .map(|_| ())
    }

    async fn lob_number<T: std::str::FromStr>(
        &mut self,
        sql: &str,
        numbers: &[i64],
    ) -> Result<T, DbError> {
        let values: Vec<_> = numbers.iter().copied().map(BoundValue::Int).collect();
        let cell = first_cell(self.single_row(sql, &values).await?)?;
        std::str::from_utf8(&cell)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| connection_error("Invalid PostgreSQL large object number"))
    }
}

fn first_cell(row: Vec<Option<Vec<u8>>>) -> Result<Vec<u8>, DbError> {
    row.into_iter()
        .next()
        .flatten()
        .ok_or_else(|| connection_error("PostgreSQL did not return a large object value"))
}

fn hex_digit(byte: u8) -> Result<u8, DbError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(10 + byte - b'a'),
        _ => Err(connection_error("Invalid PostgreSQL large object data")),
    }
}

fn file_error(cause: std::io::Error) -> DbError {
    DbError::general(cause.to_string())
}
