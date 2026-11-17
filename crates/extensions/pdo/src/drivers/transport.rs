//! Only driver-owned data crosses from the I/O runtime. PHP values are built by the worker.

use phera_extension_api::Value;

use super::postgres_values::Cell;
use super::{Rows, postgres};
use crate::errors::DbError;

type PostgresRow = (tokio_postgres::Row, Vec<(usize, String)>);

pub enum RawRows {
    Affected(u64),
    Mysql {
        columns: Vec<mysql_async::Column>,
        rows: Vec<mysql_async::Row>,
        affected: u64,
    },
    Postgres {
        columns: Vec<super::Column>,
        rows: Vec<PostgresRow>,
        affected: u64,
    },
}

impl RawRows {
    pub fn has_columns(&self) -> bool {
        match self {
            Self::Affected(_) => false,
            Self::Mysql { columns, .. } => !columns.is_empty(),
            Self::Postgres { columns, .. } => !columns.is_empty(),
        }
    }

    pub fn into_php(self) -> Result<Rows, DbError> {
        match self {
            Self::Affected(affected) => Ok(Rows {
                affected,
                ..Rows::default()
            }),
            Self::Mysql {
                columns,
                rows,
                affected,
            } => Ok(super::mysql_values::convert(columns, rows, affected)),
            Self::Postgres {
                columns,
                rows,
                affected,
            } => {
                let mut values = Vec::with_capacity(rows.len());
                for (row, formatted) in rows {
                    let mut cells = Vec::with_capacity(row.len());
                    for index in 0..row.len() {
                        let value = if let Some((_, text)) =
                            formatted.iter().find(|(original, _)| *original == index)
                        {
                            Value::from(text.clone())
                        } else {
                            let cell: Option<Cell> = row
                                .try_get(index)
                                .map_err(|error| postgres::error(error, ""))?;
                            cell.map_or(Value::Null, |cell| cell.0)
                        };
                        cells.push(value);
                    }
                    values.push(cells);
                }
                Ok(Rows {
                    columns,
                    values,
                    affected,
                })
            }
        }
    }
}
