//! SQLite binds `PARAM_BOOL` floats as integers; PDO normalizes integer inputs first.

use crate::parameters::{Binding, ParameterType};
use phera_extension_api::value::{to_bool, to_int, to_php_string};
use phera_extension_api::{Context, NativeError, Value};
use phera_sqlite::Cell;

pub fn normalize(context: &mut dyn Context, binding: &mut Binding) -> Result<(), NativeError> {
    let value = binding.value.deref().clone();
    let normalized = match (binding.kind, &value) {
        (ParameterType::String, value) if !value.is_null() => {
            Value::String(to_php_string(value, context.diagnostics())?)
        }
        (ParameterType::Int, Value::Bool(value)) => Value::Int(i64::from(*value)),
        (ParameterType::Bool, Value::Int(_)) => Value::Bool(to_bool(&value)),
        _ => value,
    };
    if let Value::Reference(reference) = &binding.value {
        *reference.borrow_mut() = normalized;
    } else {
        binding.value = normalized;
    }
    Ok(())
}

pub fn read(context: &mut dyn Context, binding: &Binding) -> Result<Cell, NativeError> {
    let value = binding.value.deref().clone();
    if value.is_null() || matches!(binding.kind, ParameterType::Null) {
        return Ok(Cell::Null);
    }
    let (cell, converted) = match binding.kind {
        ParameterType::Null => return Ok(Cell::Null),
        ParameterType::Int | ParameterType::Bool => {
            let value = to_int(&value, context.diagnostics())?;
            (Cell::Integer(value), Value::Int(value))
        }
        ParameterType::String | ParameterType::Blob => {
            let value = to_php_string(&value, context.diagnostics())?;
            let bytes = value.as_bytes().to_vec();
            let cell = if matches!(binding.kind, ParameterType::Blob) {
                Cell::Blob(bytes)
            } else {
                Cell::Text(bytes)
            };
            (cell, Value::String(value))
        }
    };
    if let Value::Reference(reference) = &binding.value {
        *reference.borrow_mut() = converted;
    }
    Ok(cell)
}

/// Converts one positional `execute([...])` parameter directly into a SQLite
/// `Cell` without allocating a `HashMap` or `Binding` wrapper.
pub fn read_execute_value(context: &mut dyn Context, value: &Value) -> Result<Cell, NativeError> {
    let value = value.deref();
    if value.is_null() {
        return Ok(Cell::Null);
    }
    let text = to_php_string(&value, context.diagnostics())?;
    Ok(Cell::Text(text.as_bytes().to_vec()))
}
