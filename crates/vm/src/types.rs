//! Scalar declaration checks delegate conversions to the value layer.
use super::*;
use phera_value::{is_numeric, to_float, to_int};
pub(crate) fn coerce(value: Value, ty: &str, strict: bool, vm: &mut Vm) -> NativeResult {
    let v = value.deref().clone();
    let nullable = ty.starts_with('?');
    let ty = ty.trim_start_matches('?');
    let parts: Vec<&str> = ty.split('|').collect();
    if (nullable || parts.contains(&"null")) && v.is_null() {
        return Ok(v);
    }
    for t in &parts {
        if match *t {
            "mixed" => true,
            "null" | "void" => v.is_null(),
            "int" => v.is_int(),
            "float" => v.is_float(),
            "bool" => v.is_bool(),
            "true" => matches!(v, Value::Bool(true)),
            "false" => matches!(v, Value::Bool(false)),
            "string" => v.is_string(),
            "array" | "iterable" => v.is_array(),
            "callable" => {
                if let Value::String(s) = &v {
                    vm.exists(&s.to_string_lossy())
                } else {
                    false
                }
            }
            _ => false,
        } {
            return Ok(v);
        }
    }
    let mut d = CollectDiagnostics::default();
    if parts.contains(&"float") && v.is_int() {
        return Ok(Value::Float(to_float(&v, &mut d)?));
    }
    if !strict
        && !v.is_null()
        && !v.is_array()
        && !v.is_object()
        && !matches!(v, Value::Resource(_))
    {
        let prefer_float = parts.contains(&"int")
            && parts.contains(&"float")
            && matches!(&v,Value::String(s) if matches!(phera_value::parse_numeric(s.as_bytes()).value,Some(phera_value::NumVal::Float(_))));
        for t in if prefer_float {
            ["float", "int", "string", "bool"]
        } else {
            ["int", "float", "string", "bool"]
        } {
            if !parts.contains(&t) {
                continue;
            }
            let numeric = match &v {
                Value::String(s) => is_numeric(s.as_bytes()),
                _ => true,
            };
            let result = match t {
                "int" if numeric => Some(Value::Int(to_int(&v, &mut d)?)),
                "float" if numeric => Some(Value::Float(to_float(&v, &mut d)?)),
                "string" => Some(Value::String(to_php_string(&v, &mut d)?)),
                "bool" => Some(Value::Bool(to_bool(&v))),
                _ => None,
            };
            if let Some(v) = result {
                vm.diagnostics_flush(d);
                return Ok(v);
            }
        }
    }
    Err(NativeError::type_error(format!(
        "must be of type {}, {} given",
        if nullable {
            format!("?{ty}")
        } else {
            ty.into()
        },
        v.type_name()
    )))
}
