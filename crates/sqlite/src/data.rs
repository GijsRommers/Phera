//! SQLite values preserve bytes: `SELECT X'00FF', 9223372036854775807, NULL`.

use rusqlite::types::{ToSql, ToSqlOutput, ValueRef};

#[derive(Clone, Debug)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

impl Cell {
    pub fn kind(&self) -> i64 {
        match self {
            Self::Integer(_) => 1,
            Self::Real(_) => 2,
            Self::Text(_) => 3,
            Self::Blob(_) => 4,
            Self::Null => 5,
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
    pub columns: Vec<Column>,
    pub parameters: Vec<Option<String>>,
    pub readonly: bool,
    pub explain: i32,
}

impl Description {
    pub(crate) fn read(statement: &rusqlite::Statement<'_>) -> Self {
        let metadata: Vec<_> = statement.columns_with_metadata().collect();
        Self {
            columns: statement
                .columns()
                .enumerate()
                .map(|(index, column)| Column {
                    name: column.name().into(),
                    declared: column.decl_type().map(str::to_owned),
                    table: metadata[index].table_name().map(str::to_owned),
                })
                .collect(),
            parameters: (1..=statement.parameter_count())
                .map(|index| statement.parameter_name(index).map(str::to_owned))
                .collect(),
            readonly: statement.readonly(),
            explain: statement.is_explain(),
        }
    }
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
            code: 21,
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
                code: 25,
                message: "column index out of range".into(),
            },
            other => Self {
                code: 1,
                message: other.to_string(),
            },
        }
    }
}
