//! Convert PostgreSQL text to PDO cells as each DataRow arrives.

use phera_extension_api::Value;
use postgres_protocol::message::backend::DataRowBody;
use postgres_types::Type;

use super::wire::rows::{Cells, Field};
use crate::drivers::{Column, RowBuffer, connection_error};
use crate::errors::DbError;
use crate::metadata::{Metadata, PostgresColumn};

pub(super) fn column(field: Field) -> Column {
    Column {
        name: field.name,
        metadata: Metadata::Postgres(PostgresColumn {
            oid: field.oid,
            table_oid: field.table,
            modifier: field.modifier,
            native_type: type_name(field.oid),
        }),
    }
}

pub(super) fn type_name(oid: u32) -> String {
    Type::from_oid(oid).map_or_else(|| oid.to_string(), |kind| kind.name().to_owned())
}

pub(super) fn append(
    columns: &[Column],
    values: &mut RowBuffer,
    row: &DataRowBody,
) -> Result<(), DbError> {
    let mut cells = Cells::new(row);
    if cells.len() != columns.len() {
        return Err(connection_error(
            "PostgreSQL row width differs from its description",
        ));
    }
    values.try_push(columns.iter().map(|column| {
        let Some(bytes) = cells.next().map_err(connection_error)? else {
            return Ok(Value::Null);
        };
        let Metadata::Postgres(ref metadata) = column.metadata else {
            return Err(connection_error("Mismatched metadata driver"));
        };
        decode(metadata.oid, bytes)
    }))?;
    cells.finish().map_err(connection_error)
}

fn decode(oid: u32, bytes: &[u8]) -> Result<Value, DbError> {
    let value = match Type::from_oid(oid) {
        Some(Type::BOOL) => match bytes {
            b"t" => Value::Bool(true),
            b"f" => Value::Bool(false),
            _ => return Err(connection_error("Invalid PostgreSQL boolean")),
        },
        Some(Type::INT2 | Type::INT4 | Type::INT8 | Type::OID) => {
            let value = std::str::from_utf8(bytes).map_err(connection_error)?;
            Value::Int(value.parse().map_err(connection_error)?)
        }
        Some(Type::FLOAT4 | Type::FLOAT8) => {
            let value = std::str::from_utf8(bytes).map_err(connection_error)?;
            Value::Float(value.parse().map_err(connection_error)?)
        }
        Some(Type::BYTEA) => {
            return Err(DbError::general(
                "PostgreSQL bytea streams are not supported yet",
            ));
        }
        _ => Value::from(bytes.to_vec()),
    };
    Ok(value)
}
