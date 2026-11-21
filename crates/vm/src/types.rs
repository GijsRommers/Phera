//! Scalar declaration checks delegate conversions to the value layer.

use phera_bytecode::{Type, TypeMask};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_bool, to_php_string};
use phera_value::{is_numeric, to_float, to_int};

use crate::Vm;

pub(crate) use phera_bytecode::DeclarationScope;

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

/// The bit of the built-in type a value has itself.
pub(crate) fn own_type(value: &Value) -> TypeMask {
    match value {
        Value::Null => TypeMask::NULL,
        Value::Bool(true) => TypeMask::TRUE,
        Value::Bool(false) => TypeMask::FALSE,
        Value::Int(_) => TypeMask::INT,
        Value::Float(_) => TypeMask::FLOAT,
        Value::String(_) => TypeMask::STRING,
        Value::Array(_) => TypeMask::ARRAY,
        Value::Object(_) => TypeMask::OBJECT,
        _ => TypeMask::default(),
    }
}

/// Whether a value passes a declared type exactly as it is: `5` for `int`,
/// `"a"` for `?string`, anything for `mixed`.
///
/// This is the usual case, and deciding it takes one bit test: no class
/// lookup, no conversion, no error message. Every type check asks this first.
/// A PHP reference is not accepted here, because the check may have to write
/// a converted value back through it.
#[inline]
pub(crate) fn accepts_as_is(value: &Value, declaration: &Type) -> bool {
    !matches!(value, Value::Reference(_)) && declaration.mask.intersects(own_type(value))
}

/// PHP accepts an int wherever `float` is declared and `int` is not, also
/// under `strict_types`: `function f(float $x)` called as `f(5)` gets `5.0`.
/// Returns the float, or `None` when this rule does not apply.
#[inline]
pub(crate) fn widen_int_to_float(value: &Value, mask: TypeMask) -> Option<Value> {
    let Value::Int(integer) = value else {
        return None;
    };
    (mask.intersects(TypeMask::FLOAT) && !mask.intersects(TypeMask::INT))
        .then_some(Value::Float(*integer as f64))
}

/// Apply the declaration's PHP type policy while retaining diagnostic ordering.
pub(crate) fn coerce(
    value: Value,
    declaration_type: &Type,
    checking: TypeChecking,
    vm: &mut Vm,
) -> NativeResult {
    coerce_in_scope(value, declaration_type, checking, vm, None)
}

/// Callable declarations use the declaring function's scope, even during argument binding.
pub(crate) fn coerce_in_scope(
    value: Value,
    declaration_type: &Type,
    checking: TypeChecking,
    vm: &mut Vm,
    scope: Option<&crate::classes::MethodContext>,
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
    if mask.intersects(own_type(&value)) {
        return Ok(value);
    }
    if vm.matches_class_type(&value, declaration_type) {
        return Ok(value);
    }
    if mask.intersects(TypeMask::VOID) && value.is_null()
        || mask.intersects(TypeMask::ITERABLE) && value.is_iterable()
    {
        return Ok(value);
    }
    if mask.intersects(TypeMask::CALLABLE)
        && vm
            .resolve_callable_for(&value, crate::callables::CallableUse::Type(scope))
            .is_ok()
    {
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
    let display = php_type_display(declaration_type);
    let declared = diagnostic_type(&display);
    Err(NativeError::type_error(format!(
        "must be of type {}, {} given",
        declared,
        match &value {
            Value::Object(object) => object.class.name().to_owned(),
            _ => value.type_name(),
        }
    )))
}

/// PHP prints the anonymous class prefix in type errors, while matching its whole identity.
fn diagnostic_type(declaration: &str) -> std::borrow::Cow<'_, str> {
    if !declaration.contains('\0') {
        return declaration.into();
    }
    let separators = ['|', '&', '?', '(', ')'];
    let mut display = String::new();
    for segment in declaration.split_inclusive(separators) {
        let name = segment.trim_end_matches(separators);
        display.push_str(name.split('\0').next().unwrap_or(name));
        display.push_str(&segment[name.len()..]);
    }
    display.into()
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
        // Only the whole `bool` type converts a value to a boolean. `false`
        // or `true` alone in a union (`int|false`) accepts that one value;
        // the string "x" is not turned into `true` for it.
        let offered = if candidate == "bool" {
            mask.contains(TypeMask::BOOL)
        } else {
            mask.intersects(wanted)
        };
        if !offered {
            continue;
        }
        let numeric = match value {
            Value::String(string) => is_numeric(string.as_bytes()),
            _ => true,
        };
        let result = match candidate {
            "int" if numeric => weak_int(value, diagnostics)?,
            "float" if numeric => Some(Value::Float(to_float(value, diagnostics)?)),
            "string" => Some(Value::String(to_php_string(value, diagnostics)?)),
            "bool" => {
                if matches!(value, Value::Float(float) if float.is_nan()) {
                    phera_value::Diagnostics::warning(
                        diagnostics,
                        "unexpected NAN value was coerced to bool".to_owned(),
                    );
                }
                Some(Value::Bool(to_bool(value)))
            }
            _ => None,
        };
        if let Some(value) = result {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// The integer a value becomes for an `int` declaration without
/// `strict_types`, or `None` when it cannot become one.
///
/// ```php
/// function f(int $n) {}
/// f("7");    // 7
/// f(2.0);    // 2
/// f(1.5);    // 1, with "Implicit conversion from float 1.5 to int loses precision"
/// f(1e30);   // TypeError: no integer is that large
/// f(NAN);    // TypeError
/// ```
///
/// This is not the `(int)` cast, which accepts every float and wraps
/// around.
fn weak_int(
    value: &Value,
    diagnostics: &mut phera_value::CollectDiagnostics,
) -> Result<Option<Value>, NativeError> {
    use phera_value::{Diagnostics, NumVal};

    /// 2 to the power 63: the first float above every integer.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;

    let (float, written_as) = match value {
        Value::Float(float) => (*float, None),
        Value::String(string) => match phera_value::parse_numeric(string.as_bytes()).value {
            Some(NumVal::Float(float)) => (float, Some(string)),
            _ => return Ok(Some(Value::Int(to_int(value, diagnostics)?))),
        },
        _ => return Ok(Some(Value::Int(to_int(value, diagnostics)?))),
    };
    if !(-LIMIT..LIMIT).contains(&float) {
        // Too large, infinite or not a number.
        return Ok(None);
    }
    let int = float as i64;
    if int as f64 != float {
        diagnostics.deprecated(match written_as {
            Some(string) => format!(
                "Implicit conversion from float-string \"{}\" to int loses precision",
                string.to_string_lossy()
            ),
            None => format!(
                "Implicit conversion from float {} to int loses precision",
                phera_value::float_repr(float)
            ),
        });
    }
    Ok(Some(Value::Int(int)))
}

/// PHP expands its iterable alias when reporting a type mismatch:
/// `iterable|float` is reported as `Traversable|array|float`.
pub(crate) fn php_type_display(declaration: &Type) -> std::borrow::Cow<'_, str> {
    if !declaration.mask.intersects(TypeMask::ITERABLE) {
        return declaration.display.as_str().into();
    }
    expanded_iterable_display(&declaration.display).into()
}

pub(crate) fn expanded_iterable_display(display: &str) -> String {
    let nullable = display.starts_with('?');
    let expanded = display
        .trim_start_matches('?')
        .split('|')
        .flat_map(|name| match name {
            "iterable" => ["Traversable", "array"],
            name => [name, ""],
        })
        .chain(nullable.then_some("null"))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join("|");
    phera_bytecode::union_in_php_order(&expanded)
}
