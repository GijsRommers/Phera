//! `bindParam(':id', $id, PDO::PARAM_INT)` reads the current referenced value on execute.

use std::collections::HashMap;

use phera_extension_api::value::{to_bool, to_int, to_php_string};
use phera_extension_api::{ArrayKey, Context, NativeError, Value};

use crate::constants::{PARAM_BOOL, PARAM_INT, PARAM_NULL, PARAM_STR};
use crate::errors::{DbError, unsupported};
use crate::placeholders::{Parameter, Sql};

#[derive(Clone, Copy)]
pub enum ParameterType {
    Null,
    Int,
    String,
    Bool,
}

impl ParameterType {
    pub fn parse(value: i64) -> Result<Self, NativeError> {
        match value {
            PARAM_NULL => Ok(Self::Null),
            PARAM_INT => Ok(Self::Int),
            PARAM_STR => Ok(Self::String),
            PARAM_BOOL => Ok(Self::Bool),
            _ => Err(unsupported("This PDO parameter type")),
        }
    }
}

#[derive(Clone)]
pub struct Binding {
    pub value: Value,
    pub kind: ParameterType,
}

#[derive(Clone, Debug)]
pub enum BoundValue {
    Null,
    Int(i64),
    String(Vec<u8>),
    Bool(bool),
}

impl Binding {
    pub fn read(&self, context: &mut dyn Context) -> Result<BoundValue, NativeError> {
        let value = self.value.deref();
        if value.is_null() {
            return Ok(BoundValue::Null);
        }
        match self.kind {
            ParameterType::Null => Ok(BoundValue::Null),
            ParameterType::Int => Ok(BoundValue::Int(to_int(&value, context.diagnostics())?)),
            ParameterType::Bool => Ok(BoundValue::Bool(to_bool(&value))),
            ParameterType::String => Ok(BoundValue::String(
                to_php_string(&value, context.diagnostics())?
                    .as_bytes()
                    .to_vec(),
            )),
        }
    }
}

pub fn key(value: &Value, method: Option<&str>) -> Result<String, NativeError> {
    match &*value.deref() {
        Value::Int(number) => {
            let position = if method.is_none() {
                *number
            } else {
                number.saturating_sub(1)
            };
            if position < 0 {
                return Err(NativeError::value_error(format!(
                    "{}(): Argument #1 ($param) must be greater than or equal to 1",
                    method.unwrap_or("PDOStatement::execute")
                )));
            }
            Ok(position.to_string())
        }
        Value::String(name) => {
            if name.as_bytes().is_empty()
                && let Some(method) = method
            {
                return Err(NativeError::value_error(format!(
                    "{method}(): Argument #1 ($param) must not be empty"
                )));
            }
            Ok(name.to_string_lossy().trim_start_matches(':').to_owned())
        }
        _ => Err(NativeError::type_error(
            "Parameter must be an integer or string",
        )),
    }
}

pub fn execute_bindings(params: &Value) -> Result<HashMap<String, Binding>, NativeError> {
    let mut bindings = HashMap::new();
    if let Value::Array(array) = &*params.deref() {
        for (param, value) in array.iter() {
            let name = match param {
                ArrayKey::Int(position) => key(&Value::Int(position), None)?,
                ArrayKey::String(name) => name.to_string_lossy().trim_start_matches(':').to_owned(),
            };
            bindings.insert(
                name,
                Binding {
                    value: value.clone(),
                    kind: ParameterType::String,
                },
            );
        }
    }
    Ok(bindings)
}

pub fn ordered<'a>(
    sql: &Sql,
    bindings: &'a HashMap<String, Binding>,
) -> Result<Vec<&'a Binding>, DbError> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for parameter in &sql.parameters {
        let name = match parameter {
            Parameter::Name(name) => name.clone(),
            Parameter::Position(position) => position.to_string(),
        };
        seen.insert(name.clone());
        result.push(bindings.get(&name).ok_or_else(|| {
            let count = sql
                .parameters
                .iter()
                .map(|parameter| match parameter {
                    Parameter::Name(name) => name.clone(),
                    Parameter::Position(position) => position.to_string(),
                })
                .collect::<std::collections::HashSet<_>>()
                .len();
            DbError::parameter(if count == bindings.len() {
                "parameter was not defined"
            } else {
                "number of bound variables does not match number of tokens"
            })
        })?);
    }
    if seen.len() != bindings.len() {
        return Err(DbError::parameter(
            "number of bound variables does not match number of tokens",
        ));
    }
    Ok(result)
}

pub fn literal(value: &BoundValue, mysql: bool) -> String {
    match value {
        BoundValue::Null => "NULL".into(),
        BoundValue::Int(value) => value.to_string(),
        BoundValue::Bool(value) => match (mysql, value) {
            (true, true) => "1",
            (true, false) => "0",
            (false, true) => "true",
            (false, false) => "false",
        }
        .into(),
        BoundValue::String(bytes) => quote(bytes, mysql),
    }
}

pub fn quote(bytes: &[u8], mysql: bool) -> String {
    let mut quoted = String::from("'");
    for character in String::from_utf8_lossy(bytes).chars() {
        match character {
            '\'' => quoted.push_str(if mysql { "\\'" } else { "''" }),
            '\\' if mysql => quoted.push_str("\\\\"),
            '\0' if mysql => quoted.push_str("\\0"),
            '\n' if mysql => quoted.push_str("\\n"),
            '\r' if mysql => quoted.push_str("\\r"),
            '\u{1a}' if mysql => quoted.push_str("\\Z"),
            _ => quoted.push(character),
        }
    }
    quoted.push('\'');
    quoted
}
