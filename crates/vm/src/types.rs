//! Scalar declaration checks delegate conversions to the value layer.
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_bool, to_php_string};

use crate::Vm;
use phera_bytecode::{Type, TypeMask};
use phera_value::{is_numeric, to_float, to_int};
pub(crate) fn coerce(value: Value, ty: &Type, strict: bool, vm: &mut Vm) -> NativeResult {
    let value = value.deref().clone();
    let mask = ty.mask;
    let actual = match &value {
        Value::Null => TypeMask::NULL,
        Value::Bool(true) => TypeMask::TRUE,
        Value::Bool(false) => TypeMask::FALSE,
        Value::Int(_) => TypeMask::INT,
        Value::Float(_) => TypeMask::FLOAT,
        Value::String(_) => TypeMask::STRING,
        Value::Array(_) => TypeMask::ARRAY,
        Value::Object(_) => TypeMask::OBJECT,
        _ => TypeMask::default(),
    };
    if mask.intersects(actual)
        || mask.intersects(TypeMask::VOID) && value.is_null()
        || mask.intersects(TypeMask::ITERABLE) && value.is_array()
    {
        return Ok(value);
    }
    if mask.intersects(TypeMask::CALLABLE)
        && let Value::String(name) = &value
        && vm.exists(&name.to_string_lossy())
    {
        return Ok(value);
    }
    let mut diagnostics = std::mem::take(&mut vm.diagnostic_reporter.operator_diagnostics);
    if mask.intersects(TypeMask::FLOAT) && value.is_int() {
        return Ok(Value::Float(to_float(&value, &mut diagnostics)?));
    }
    if !strict
        && !value.is_null()
        && !value.is_array()
        && !value.is_object()
        && !matches!(value, Value::Resource(_))
    {
        let prefer_float = mask.intersects(TypeMask::INT)
            && mask.intersects(TypeMask::FLOAT)
            && matches!(&value,Value::String(s) if matches!(phera_value::parse_numeric(s.as_bytes()).value,Some(phera_value::NumVal::Float(_))));
        for candidate in if prefer_float {
            ["float", "int", "string", "bool"]
        } else {
            ["int", "float", "string", "bool"]
        } {
            let wanted = match candidate {
                "int" => TypeMask::INT,
                "float" => TypeMask::FLOAT,
                "string" => TypeMask::STRING,
                _ => TypeMask::BOOL,
            };
            if !mask.intersects(wanted) {
                continue;
            }
            let numeric = match &value {
                Value::String(s) => is_numeric(s.as_bytes()),
                _ => true,
            };
            let result = match candidate {
                "int" if numeric => Some(Value::Int(to_int(&value, &mut diagnostics)?)),
                "float" if numeric => Some(Value::Float(to_float(&value, &mut diagnostics)?)),
                "string" => Some(Value::String(to_php_string(&value, &mut diagnostics)?)),
                "bool" => Some(Value::Bool(to_bool(&value))),
                _ => None,
            };
            if let Some(value) = result {
                vm.diagnostics_flush(diagnostics);
                return Ok(value);
            }
        }
    }
    Err(NativeError::type_error(format!(
        "must be of type {}, {} given",
        ty.display,
        value.type_name()
    )))
}
