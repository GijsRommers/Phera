//! PostgreSQL wire parameter serialization and PHP-style server error formatting.

use crate::drivers::connection_error;
use crate::errors::DbError;
use crate::parameters::BoundValue;
use bytes::BytesMut;
use std::error::Error;
use tokio_postgres::types::{Format, IsNull, ToSql, Type};

#[derive(Debug)]
pub struct TextParameter(Option<Vec<u8>>);

impl From<&BoundValue> for TextParameter {
    fn from(value: &BoundValue) -> Self {
        Self(match value {
            BoundValue::Null => None,
            BoundValue::String(value) => Some(value.clone()),
            BoundValue::Int(value) => Some(value.to_string().into_bytes()),
            BoundValue::Bool(value) => Some(if *value { b"t" } else { b"f" }.to_vec()),
        })
    }
}

impl ToSql for TextParameter {
    fn to_sql(
        &self,
        _: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match &self.0 {
            None => Ok(IsNull::Yes),
            Some(bytes) => {
                output.extend_from_slice(bytes);
                Ok(IsNull::No)
            }
        }
    }
    fn accepts(_: &Type) -> bool {
        true
    }
    fn encode_format(&self, _: &Type) -> Format {
        Format::Text
    }
    fn to_sql_checked(
        &self,
        ty: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        self.to_sql(ty, output)
    }
}

pub fn error(error: tokio_postgres::Error, query: &str) -> DbError {
    let Some(database) = error.as_db_error() else {
        if let Some(source) = error.source() {
            let detail = source.to_string();
            if detail.contains("not supported yet") {
                return DbError {
                    state: "HY000".into(),
                    code: None,
                    detail,
                };
            }
        }
        return connection_error(error);
    };
    let mut detail = format!("{}:  {}", database.severity(), database.message());
    if let Some(tokio_postgres::error::ErrorPosition::Original(position)) = database.position() {
        let offset = (*position as usize).saturating_sub(1);
        let prefix: String = query.chars().take(offset).collect();
        let line_number = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count();
        let line = query.lines().nth(line_number - 1).unwrap_or_default();
        let label = format!("LINE {line_number}: ");
        detail.push_str(&format!(
            "\n{label}{line}\n{}^",
            " ".repeat(label.len() + column)
        ));
    }
    if let Some(extra) = database.detail() {
        detail.push_str(&format!("\nDETAIL:  {extra}"));
    }
    if let Some(hint) = database.hint() {
        detail.push_str(&format!("\nHINT:  {hint}"));
    }
    DbError {
        state: database.code().code().into(),
        code: Some(7),
        detail,
    }
}
