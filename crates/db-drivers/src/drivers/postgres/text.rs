//! Let PostgreSQL render session-sensitive values (TimeZone, DateStyle, IntervalStyle).
//! tokio-postgres requests binary results. For columns whose PDO value is server
//! text, a typed query formats a group of binary rows. The original query is
//! never executed twice. Ordinary PDO numeric/text queries skip formatting.

use std::error::Error;
use std::fmt::Write;

use bytes::BytesMut;
use tokio_postgres::types::{FromSql, IsNull, Kind, ToSql, Type};
use tokio_postgres::{Client, Row};

use super::{Row as PdoRow, error};
use crate::errors::DbError;

type DecodeError = Box<dyn Error + Send + Sync>;

// 64 rows keep ordinary application pages in one round trip and bound the
// amount of SQL and parameter data built for each formatting query.
const ROWS_PER_FORMAT_QUERY: usize = 64;

#[derive(Debug)]
struct Binary<'a>(&'a [u8]);

impl<'a> FromSql<'a> for Binary<'a> {
    fn from_sql(_: &Type, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        Ok(Self(bytes))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

impl ToSql for Binary<'_> {
    fn to_sql(&self, _: &Type, output: &mut BytesMut) -> Result<IsNull, DecodeError> {
        output.extend_from_slice(self.0);
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

pub(super) async fn convert(
    client: &Client,
    rows: Vec<Row>,
    source: &str,
) -> Result<Vec<PdoRow>, DbError> {
    let Some(first) = rows.first() else {
        return Ok(Vec::new());
    };
    let columns: Vec<_> = first
        .columns()
        .iter()
        .enumerate()
        .filter(|(_, column)| server_text(column.type_()))
        .map(|(index, column)| (index, column.type_().clone()))
        .collect();
    if columns.is_empty() {
        return Ok(rows
            .into_iter()
            .map(|binary| PdoRow {
                binary,
                text: Vec::new(),
            })
            .collect());
    }
    let formatted = format(client, &rows, &columns, source).await?;
    Ok(rows
        .into_iter()
        .zip(formatted)
        .map(|(binary, row)| PdoRow {
            binary,
            text: columns
                .iter()
                .zip(row)
                .filter_map(|((index, _), value)| value.map(|text| (*index, text)))
                .collect(),
        })
        .collect())
}

/// pgsql represents every result column as text.
pub(super) async fn format_native(
    client: &Client,
    rows: &[Row],
) -> Result<Vec<Vec<Option<Vec<u8>>>>, DbError> {
    let Some(first) = rows.first() else {
        return Ok(Vec::new());
    };
    let columns: Vec<_> = first
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| (index, column.type_().clone()))
        .collect();
    Ok(format(client, rows, &columns, "")
        .await?
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|text| text.map(String::into_bytes))
                .collect()
        })
        .collect())
}

async fn format(
    client: &Client,
    rows: &[Row],
    columns: &[(usize, Type)],
    source: &str,
) -> Result<Vec<Vec<Option<String>>>, DbError> {
    let width = columns.len();
    let group_size = ROWS_PER_FORMAT_QUERY.min(usize::from(u16::MAX) / width);
    let mut formatted = Vec::with_capacity(rows.len());
    for group in rows.chunks(group_size) {
        let mut values = Vec::with_capacity(group.len() * width);
        for row in group {
            for (index, _) in columns {
                values.push(
                    row.try_get::<_, Option<Binary<'_>>>(*index)
                        .map_err(|cause| error(cause, source))?,
                );
            }
        }
        if values.iter().all(Option::is_none) {
            formatted.extend((0..group.len()).map(|_| vec![None; width]));
            continue;
        }
        let sql = format_query(group.len(), columns);
        let parameters: Vec<_> = values
            .iter()
            .zip(columns.iter().cycle())
            .map(|(value, (_, kind))| (value as &(dyn ToSql + Sync), kind.clone()))
            .collect();
        let reply = client
            .query_typed(&sql, &parameters)
            .await
            .map_err(|cause| error(cause, &sql))?;
        for row in reply {
            let cells = (0..width)
                .map(|index| {
                    row.try_get::<_, Option<String>>(index)
                        .map_err(|cause| error(cause, &sql))
                })
                .collect::<Result<_, _>>()?;
            formatted.push(cells);
        }
    }
    Ok(formatted)
}

fn format_query(row_count: usize, columns: &[(usize, Type)]) -> String {
    let mut sql = String::from("SELECT ");
    for (index, (_, kind)) in columns.iter().enumerate() {
        if index > 0 {
            sql.push(',');
        }
        let output = match *kind {
            Type::BOOL => Some("pg_catalog.boolout"),
            Type::BPCHAR => Some("pg_catalog.bpcharout"),
            Type::INET => Some("pg_catalog.inet_out"),
            _ => None,
        };
        let (prefix, position) = if row_count == 1 {
            ("$", index + 1)
        } else {
            ("col", index)
        };
        if let Some(output) = output {
            let _ = write!(sql, "{output}({prefix}{position})::text");
        } else {
            let _ = write!(sql, "{prefix}{position}::text");
        }
    }
    if row_count == 1 {
        return sql;
    }
    sql.push_str(" FROM (VALUES ");
    for row in 0..row_count {
        if row > 0 {
            sql.push(',');
        }
        let _ = write!(sql, "({row}");
        for column in 0..columns.len() {
            let _ = write!(sql, ",${}", row * columns.len() + column + 1);
        }
        sql.push(')');
    }
    sql.push_str(") AS formatted(ordinal");
    for index in 0..columns.len() {
        let _ = write!(sql, ",col{index}");
    }
    sql.push_str(") ORDER BY ordinal");
    sql
}
