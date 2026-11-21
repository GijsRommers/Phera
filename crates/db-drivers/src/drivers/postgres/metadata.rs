//! Catalog lookups happen when PHP explicitly asks for getColumnMeta().

use super::Postgres;
use crate::drivers::{Column, connection_error};
use crate::errors::DbError;
use crate::metadata::{Metadata, PostgresColumn};
use crate::parameters::BoundValue;

impl Postgres {
    pub async fn metadata(&mut self, column: &Column) -> Result<(i16, Option<String>), DbError> {
        let Metadata::Postgres(PostgresColumn { oid, table_oid, .. }) = &column.metadata else {
            return Err(connection_error("Mismatched metadata driver"));
        };
        let query = "SELECT t.typlen, c.relname FROM pg_type t \
            LEFT JOIN pg_class c ON c.oid=$2 WHERE t.oid=$1";
        let mut cells = self
            .single_row(
                query,
                &[
                    BoundValue::Int(i64::from(*oid)),
                    BoundValue::Int(i64::from(*table_oid)),
                ],
            )
            .await?
            .into_iter();
        let length = cells
            .next()
            .flatten()
            .ok_or_else(|| connection_error("PostgreSQL did not return a type length"))?;
        let length = std::str::from_utf8(&length)
            .map_err(connection_error)?
            .parse()
            .map_err(connection_error)?;
        let table = cells
            .next()
            .flatten()
            .map(String::from_utf8)
            .transpose()
            .map_err(connection_error)?;
        Ok((length, table))
    }
}
