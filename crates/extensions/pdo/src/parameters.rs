//! `bindParam(':id', $id, PDO::PARAM_INT)` reads the current referenced value on execute.

use std::collections::HashMap;
use std::rc::Rc;

use phera_extension_api::value::{to_bool, to_int, to_php_string};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

use crate::constants::{PARAM_BOOL, PARAM_INT, PARAM_LOB, PARAM_NULL, PARAM_STR};
use crate::errors::{DbError, unsupported};
use crate::placeholders::{Parameter, Sql};

#[derive(Clone, Copy)]
pub enum ParameterType {
    Null,
    Int,
    String,
    /// `PARAM_LOB`: bytes, which SQLite stores as a BLOB, not as text.
    Blob,
    Bool,
}

impl ParameterType {
    pub fn parse(value: i64) -> Result<Self, NativeError> {
        match value {
            PARAM_NULL => Ok(Self::Null),
            PARAM_INT => Ok(Self::Int),
            PARAM_STR => Ok(Self::String),
            PARAM_LOB => Ok(Self::Blob),
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

pub use phera_db_drivers::parameters::BoundValue;

/// The PHP array already puts sequential emulated parameters in SQL order.
pub enum Bindings {
    Packed(Rc<PhpArray>),
    Keyed(HashMap<String, Binding>),
}

impl Default for Bindings {
    fn default() -> Self {
        Self::Keyed(HashMap::new())
    }
}

impl Bindings {
    pub fn packed(params: &Value) -> Option<Self> {
        if let Value::Array(array) = &*params.deref()
            && array.is_list()
        {
            return Some(Self::Packed(array.clone()));
        }
        None
    }

    pub fn from_execute(params: &Value) -> Result<Self, NativeError> {
        execute_bindings(params).map(Self::Keyed)
    }

    pub fn insert(&mut self, key: String, binding: Binding) {
        if let Self::Packed(array) = self {
            let bindings = array.iter().enumerate().map(|(index, (_, value))| {
                (
                    index.to_string(),
                    Binding {
                        value: value.clone(),
                        kind: ParameterType::String,
                    },
                )
            });
            *self = Self::Keyed(bindings.collect());
        }
        if let Self::Keyed(bindings) = self {
            bindings.insert(key, binding);
        }
    }
}

pub fn read_packed(
    context: &mut dyn Context,
    array: &PhpArray,
) -> Result<Vec<BoundValue>, NativeError> {
    array
        .iter()
        .map(|(_, value)| {
            let value = value.deref();
            if value.is_null() {
                Ok(BoundValue::Null)
            } else {
                Ok(BoundValue::String(
                    to_php_string(&value, context.diagnostics())?
                        .as_bytes()
                        .to_vec(),
                ))
            }
        })
        .collect()
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
            ParameterType::Blob => Ok(BoundValue::Blob(
                to_php_string(&value, context.diagnostics())?
                    .as_bytes()
                    .to_vec(),
            )),
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
        Value::InternedStr(_) | Value::String(_) => {
            let name = value.deref();
            let name = name.str_ref().expect("checked string");
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

fn execute_bindings(params: &Value) -> Result<HashMap<String, Binding>, NativeError> {
    let mut bindings = HashMap::new();
    if let Value::Array(array) = &*params.deref() {
        for (param, value) in array.iter() {
            let name = match param {
                ArrayKey::Int(position) => key(&Value::Int(position), None)?,
                ArrayKey::Interned(name) => {
                    name.to_string_lossy().trim_start_matches(':').to_owned()
                }
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
