//! Catalog lookups happen only when PHP explicitly asks for getColumnMeta().

use tokio_postgres::Client;

use super::postgres::error;
use crate::drivers::Column;
use crate::errors::DbError;
use crate::metadata::Metadata;

pub async fn get(client: &Client, column: &Column) -> Result<(i16, Option<String>), DbError> {
    let Metadata::Postgres { oid, table_oid, .. } = &column.metadata else {
        return Err(super::connection_error("Mismatched metadata driver"));
    };
    let query = "SELECT t.typlen, c.relname FROM pg_type t \
        LEFT JOIN pg_class c ON c.oid=$2 WHERE t.oid=$1";
    let row = client
        .query_one(query, &[oid, table_oid])
        .await
        .map_err(|e| error(e, query))?;
    let length: i16 = row.get(0);
    let table: Option<String> = row.get(1);
    Ok((length, table))
}
