//! SQLite values preserve bytes: `SELECT X'00FF', 9223372036854775807, NULL`.

use std::sync::Arc;

use rusqlite::types::{ToSql, ToSqlOutput, ValueRef};

#[derive(Clone, Debug)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

/// SQLite's fundamental type codes, which PHP shows as `SQLITE3_INTEGER`
/// to `SQLITE3_NULL` and uses for `getColumnMeta()`'s types.
pub mod kind {
    pub const INTEGER: i64 = 1;
    pub const FLOAT: i64 = 2;
    pub const TEXT: i64 = 3;
    pub const BLOB: i64 = 4;
    pub const NULL: i64 = 5;
}

/// SQLite result codes the crate produces itself.
pub const SQLITE_ERROR: i32 = 1;
pub const SQLITE_LOCKED: i32 = 6;
pub const SQLITE_CANTOPEN: i32 = 14;
pub const SQLITE_MISUSE: i32 = 21;
pub const SQLITE_RANGE: i32 = 25;

impl Cell {
    /// The value's fundamental type code, one of `kind`.
    pub fn kind(&self) -> i64 {
        match self {
            Self::Integer(_) => kind::INTEGER,
            Self::Real(_) => kind::FLOAT,
            Self::Text(_) => kind::TEXT,
            Self::Blob(_) => kind::BLOB,
            Self::Null => kind::NULL,
        }
    }

    pub fn from_ref(value: ValueRef<'_>) -> Self {
        match value {
            ValueRef::Null => Self::Null,
            ValueRef::Integer(value) => Self::Integer(value),
            ValueRef::Real(value) => Self::Real(value),
            ValueRef::Text(value) => Self::Text(value.to_vec()),
            ValueRef::Blob(value) => Self::Blob(value.to_vec()),
        }
    }
}

impl ToSql for Cell {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Self::Null => ValueRef::Null,
            Self::Integer(value) => ValueRef::Integer(*value),
            Self::Real(value) => ValueRef::Real(*value),
            Self::Text(value) => ValueRef::Text(value),
            Self::Blob(value) => ValueRef::Blob(value),
        }))
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub declared: Option<String>,
    pub table: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Description {
    pub columns: Arc<[Column]>,
    pub parameters: Vec<Option<String>>,
    pub readonly: bool,
    pub explain: i32,
}

impl Description {
    pub(crate) fn read(statement: &rusqlite::Statement<'_>) -> Self {
        Self {
            columns: read_columns(statement),
            parameters: (1..=statement.parameter_count())
                .map(|index| statement.parameter_name(index).map(str::to_owned))
                .collect(),
            readonly: statement.readonly(),
            explain: statement.is_explain(),
        }
    }

    /// Reuses cached column metadata when the prepared statement's schema has
    /// not changed, avoiding per-execute column C-FFI calls and heap strings.
    pub(crate) fn read_columns_cached(
        statement: &rusqlite::Statement<'_>,
        cached: &mut Arc<[Column]>,
    ) -> Self {
        let count = statement.column_count();
        let unchanged = count == cached.len()
            && cached
                .iter()
                .enumerate()
                .all(|(index, column)| statement.column_name(index) == Ok(column.name.as_str()));
        if !unchanged {
            *cached = read_columns(statement);
        }
        Self {
            columns: Arc::clone(cached),
            parameters: Vec::new(),
            readonly: statement.readonly(),
            explain: statement.is_explain(),
        }
    }
}

fn read_columns(statement: &rusqlite::Statement<'_>) -> Arc<[Column]> {
    statement
        .columns()
        .zip(statement.columns_with_metadata())
        .map(|(column, metadata)| Column {
            name: column.name().into(),
            declared: column.decl_type().map(str::to_owned),
            table: metadata.table_name().map(str::to_owned),
        })
        .collect::<Vec<_>>()
        .into()
}

#[derive(Debug)]
pub struct Execution {
    pub description: Description,
    pub first: Option<Vec<Cell>>,
    pub changes: u64,
}

#[derive(Clone, Debug)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

impl Error {
    pub fn closed() -> Self {
        Self {
            code: SQLITE_MISUSE,
            message: "database connection is closed".into(),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(code, message) => Self {
                code: code.extended_code,
                message: message.unwrap_or_else(|| code.to_string()),
            },
            rusqlite::Error::SqlInputError { error, msg, .. } => Self {
                code: error.extended_code,
                message: msg,
            },
            rusqlite::Error::InvalidParameterName(_)
            | rusqlite::Error::InvalidParameterCount(..) => Self {
                code: SQLITE_RANGE,
                message: "column index out of range".into(),
            },
            other => Self {
                code: SQLITE_ERROR,
                message: other.to_string(),
            },
        }
    }
}
