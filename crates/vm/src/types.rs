//! Scalar declaration checks delegate conversions to the value layer.

use phera_bytecode::{Type, TypeMask};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_bool, to_php_string};
use phera_value::{is_numeric, to_float, to_int};

use crate::Vm;

/// Only declarations containing relative class keywords depend on the callee scope.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclarationScope {
    Absolute,
    Relative,
}

/// This classification mirrors argument checking's keyword scan, once per declaration.
pub(crate) fn declaration_scope(declaration: &Type) -> DeclarationScope {
    if declaration
        .display
        .split(['|', '&', '?', '(', ')'])
        .any(|name| matches!(name, "self" | "parent" | "static"))
    {
        DeclarationScope::Relative
    } else {
        DeclarationScope::Absolute
    }
}

/// Strict declarations reject scalar coercion except PHP's int-to-float widening.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeChecking {
    Strict,
    Coerce,
}

impl From<bool> for TypeChecking {
    fn from(strict: bool) -> Self {
        if strict { Self::Strict } else { Self::Coerce }
    }
}

/// Apply the declaration's PHP type policy while retaining diagnostic ordering.
pub(crate) fn coerce(
    value: Value,
    declaration_type: &Type,
    checking: TypeChecking,
    vm: &mut Vm,
) -> NativeResult {
    // Mirrors deref().clone() without a redundant retain/release for already
    // owned values. Reference cells still produce a detached value snapshot.
    let value = match value {
        Value::Reference(_) => value.deref().clone(),
        _ => value,
    };
    let mask = declaration_type.mask;
    if mask == TypeMask::MIXED {
        return Ok(value);
    }
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
    if vm.matches_class_type(&value, declaration_type) {
        return Ok(value);
    }
    if mask.intersects(actual)
        || mask.intersects(TypeMask::VOID) && value.is_null()
        || mask.intersects(TypeMask::ITERABLE) && value.is_array()
    {
        return Ok(value);
    }
    if mask.intersects(TypeMask::CALLABLE) && vm.resolve_callable(&value).is_ok() {
        return Ok(value);
    }
    let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(vm);
    if mask.intersects(TypeMask::FLOAT) && value.is_int() {
        return Ok(Value::Float(to_float(&value, &mut diagnostics.buffer)?));
    }
    if checking == TypeChecking::Coerce
        && !value.is_null()
        && !value.is_array()
        && !value.is_object()
        && !matches!(value, Value::Resource(_))
        && let Some(converted) = coerce_scalar(&value, mask, &mut diagnostics.buffer)?
    {
        return Ok(converted);
    }
    Err(NativeError::type_error(format!(
        "must be of type {}, {} given",
        declaration_type.display,
        match &value {
            Value::Object(object) => object.class.name().to_owned(),
            _ => value.type_name(),
        }
    )))
}

/// PHP union coercion prefers numeric alternatives before strings and booleans.
fn coerce_scalar(
    value: &Value,
    mask: TypeMask,
    diagnostics: &mut phera_value::CollectDiagnostics,
) -> Result<Option<Value>, NativeError> {
    let prefer_float = mask.intersects(TypeMask::INT)
        && mask.intersects(TypeMask::FLOAT)
        && matches!(value,Value::String(string) if matches!(phera_value::parse_numeric(string.as_bytes()).value,Some(phera_value::NumVal::Float(_))));
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
        let numeric = match value {
            Value::String(string) => is_numeric(string.as_bytes()),
            _ => true,
        };
        let result = match candidate {
            "int" if numeric => Some(Value::Int(to_int(value, diagnostics)?)),
            "float" if numeric => Some(Value::Float(to_float(value, diagnostics)?)),
            "string" => Some(Value::String(to_php_string(value, diagnostics)?)),
            "bool" => Some(Value::Bool(to_bool(value))),
            _ => None,
        };
        if let Some(value) = result {
            return Ok(Some(value));
        }
    }
    Ok(None)
}
