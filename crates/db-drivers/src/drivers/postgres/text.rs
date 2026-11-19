//! Let PostgreSQL render session-sensitive values (TimeZone, DateStyle, IntervalStyle).
//! tokio-postgres requests binary results. For columns whose PDO value is server
//! text, one additional typed query per row asks the server to format those exact
//! binary values. The original query is never executed twice. Ordinary numeric/text
//! queries do not take this path. bytea remains unsupported until streams exist.

use std::error::Error;

use bytes::BytesMut;
use tokio_postgres::types::{FromSql, IsNull, Kind, ToSql, Type};
use tokio_postgres::{Client, Row};

use super::error;
use crate::errors::DbError;

type DecodeError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
struct Binary(Vec<u8>);

impl<'a> FromSql<'a> for Binary {
    fn from_sql(_: &Type, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        Ok(Self(bytes.to_vec()))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

impl ToSql for Binary {
    fn to_sql(&self, _: &Type, output: &mut BytesMut) -> Result<IsNull, DecodeError> {
        output.extend_from_slice(&self.0);
        Ok(IsNull::No)
    }
    fn accepts(_: &Type) -> bool {
        true
    }
    fn to_sql_checked(&self, ty: &Type, output: &mut BytesMut) -> Result<IsNull, DecodeError> {
        self.to_sql(ty, output)
    }
}

pub(super) fn server_text(kind: &Type) -> bool {
    matches!(
        *kind,
        Type::TIME
            | Type::TIMETZ
            | Type::INTERVAL
            | Type::TIMESTAMPTZ
            | Type::TIMESTAMP
            | Type::DATE
            | Type::INET
            | Type::CIDR
    ) || matches!(kind.kind(), Kind::Array(_))
}

pub async fn convert(
    client: &Client,
    row: &Row,
    source: &str,
) -> Result<Vec<(usize, String)>, DbError> {
    let mut values = Vec::new();
    let mut pending = Vec::new();
    for (index, column) in row.columns().iter().enumerate() {
        if server_text(column.type_()) {
            let raw: Option<Binary> = row.try_get(index).map_err(|e| error(e, source))?;
            if let Some(raw) = raw {
                pending.push((index, raw, column.type_().clone()));
            }
        }
    }
    if !pending.is_empty() {
        let expressions: Vec<_> = pending
            .iter()
            .enumerate()
            .map(|(index, (_, _, kind))| {
                let parameter = index + 1;
                if *kind == Type::INET {
                    format!("pg_catalog.inet_out(${parameter})::text")
                } else {
                    format!("${parameter}::text")
                }
            })
            .collect();
        let query = format!("SELECT {}", expressions.join(","));
        let parameters: Vec<_> = pending
            .iter()
            .map(|(_, raw, kind)| (raw as &(dyn ToSql + Sync), kind.clone()))
            .collect();
        let formatted = client
            .query_typed(&query, &parameters)
            .await
            .map_err(|e| error(e, &query))?;
        for (index, (original, _, _)) in pending.iter().enumerate() {
            values.push((*original, formatted[0].get::<_, String>(index)));
        }
    }
    Ok(values)
}

/// Render a whole binary row in the text formats used by pgsql.
pub async fn format_row(
    client: &Client,
    row: &Row,
    sql: &str,
) -> Result<Vec<Option<Vec<u8>>>, DbError> {
    let values: Result<Vec<Option<Binary>>, _> =
        (0..row.len()).map(|index| row.try_get(index)).collect();
    let values = values.map_err(|cause| error(cause, sql))?;
    let parameters: Vec<_> = values
        .iter()
        .zip(row.columns())
        .map(|(value, column)| (value as &(dyn ToSql + Sync), column.type_().clone()))
        .collect();
    let result = client
        .query_typed_one(sql, &parameters)
        .await
        .map_err(|cause| error(cause, sql))?;
    (0..result.len())
        .map(|index| {
            result
                .try_get::<_, Option<&str>>(index)
                .map(|text| text.map(|text| text.as_bytes().to_vec()))
                .map_err(|cause| error(cause, sql))
        })
        .collect()
}
