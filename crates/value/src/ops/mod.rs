//! PHP operators over [`Value`]. Each function sees through
//! [`Value::Reference`] and reports PHP's warnings to `&mut dyn Diagnostics`.

pub mod arithmetic;
pub mod bitwise;
pub mod compare;
pub mod concat;
pub mod incdec;

pub use arithmetic::{add, div, modulo, mul, negate, pow, sub, unary_plus};
pub use bitwise::{bitwise_and, bitwise_not, bitwise_or, bitwise_xor, shift_left, shift_right};
pub use compare::{
    LazyComparison, ObjectComparator, ObjectComparison, ObjectOperands, StringComparison,
    StringComparisonStep, compare, compare_after_error, compare_op, compare_with_objects,
    compare_with_string_objects, less, less_equal, loose_equals, loose_equals_with_objects,
    strict_equals,
};
pub use concat::concat;
pub use incdec::{decrement, increment};

use crate::convert::{NumVal, parse_numeric};
use crate::error::{Diagnostics, OpError};
use crate::value::Value;

/// Operators work on owned copies: cloning a `Value` is cheap (all
/// aggregate payloads are `Rc`). References are resolved here once.
pub(crate) fn deref_clone(v: &Value) -> Value {
    match v {
        Value::Reference(r) => r.borrow().clone(),
        other => other.clone(),
    }
}

/// `zend_binop_error`: PHP's TypeError for mismatched operand types.
pub fn binop_type_error(sigil: &str, a: &Value, b: &Value) -> OpError {
    OpError::type_error(format!(
        "Unsupported operand types: {} {} {}",
        a.type_name(),
        sigil,
        b.type_name()
    ))
}

/// Conversion failure marker: the caller builds PHP's TypeError, naming the
/// original operands which it alone knows in the right order.
pub struct ConvFail;

/// `_zendi_try_convert_scalar_to_number`: null/bool become ints, strings
/// become numbers (warning on trailing data), everything else fails.
pub(crate) fn to_number(v: &Value, diag: &mut dyn Diagnostics) -> Result<NumVal, ConvFail> {
    match &v {
        Value::Int(i) => Ok(NumVal::Int(*i)),
        Value::Float(f) => Ok(NumVal::Float(*f)),
        Value::Bool(b) => Ok(NumVal::Int(*b as i64)),
        Value::Null => Ok(NumVal::Int(0)),
        Value::String(s) => {
            let p = parse_numeric(s.as_bytes());
            match p.value {
                None => Err(ConvFail),
                Some(val) => {
                    if !p.exact {
                        diag.warning("A non-numeric value encountered".to_string());
                    }
                    Ok(val)
                }
            }
        }
        _ => Err(ConvFail),
    }
}

/// Converts both operands, raising PHP's TypeError with the original
/// operand order when either side fails (left side first, like PHP).
pub(crate) fn to_number2(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<(NumVal, NumVal), OpError> {
    // Sequential: PHP stops at the first failing side.
    let x = to_number(a, diag).map_err(|_| binop_type_error(sigil, a, b))?;
    let y = to_number(b, diag).map_err(|_| binop_type_error(sigil, a, b))?;
    Ok((x, y))
}

/// `zendi_try_get_long`: int-like coercion used by `%`, shifts and bitwise
/// ops. `Err` means PHP's TypeError path.
pub fn try_get_long(v: &Value, diag: &mut dyn Diagnostics) -> Result<i64, ConvFail> {
    match &v {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(*b as i64),
        Value::Null => Ok(0),
        Value::Float(f) => Ok(crate::convert::float_to_key_int(*f, diag)),
        Value::String(s) => {
            let p = parse_numeric(s.as_bytes());
            match p.value {
                None => Err(ConvFail),
                Some(NumVal::Int(i)) => {
                    if !p.exact {
                        diag.warning("A non-numeric value encountered".to_string());
                    }
                    Ok(i)
                }
                Some(NumVal::Float(f)) => {
                    if !p.exact {
                        diag.warning("A non-numeric value encountered".to_string());
                    }
                    let l = float_string_cap(f);
                    if l as f64 != f {
                        diag.deprecated(format!(
                            "Implicit conversion from float-string \"{}\" to int loses precision",
                            String::from_utf8_lossy(s.as_bytes())
                        ));
                    }
                    Ok(l)
                }
            }
        }
        _ => Err(ConvFail),
    }
}

/// Both operands through `zendi_try_get_long`, PHP's TypeError on failure
/// with the original operand order.
pub(crate) fn try_get_long2(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<(i64, i64), OpError> {
    let x = try_get_long(a, diag).map_err(|_| binop_type_error(sigil, a, b))?;
    let y = try_get_long(b, diag).map_err(|_| binop_type_error(sigil, a, b))?;
    Ok((x, y))
}

/// `zend_dval_to_lval_cap`: string-float to int silently clamps.
pub(crate) fn float_string_cap(d: f64) -> i64 {
    match d {
        d if !d.is_finite() => 0,
        d if d >= 9223372036854775808.0 => i64::MAX,
        d if d < -9223372036854775808.0 => i64::MIN,
        d => d as i64,
    }
}

/// Overflowing int product: returns `(low_bits, exact_product_as_double,
/// overflowed)` like `ZEND_SIGNED_MULTIPLY_LONG`.
pub(crate) fn signed_mul(a: i64, b: i64) -> (i64, f64, bool) {
    match a.checked_mul(b) {
        Some(p) => (p, p as f64, false),
        None => (a.wrapping_mul(b), a as f64 * b as f64, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::array::PhpArray;
    use crate::error::{CollectDiagnostics, NoDiagnostics};
    use std::cell::RefCell;

    fn r(v: Value) -> Value {
        Value::Reference(std::rc::Rc::new(RefCell::new(v)))
    }

    #[test]
    fn operators_see_through_references() {
        let mut d = CollectDiagnostics::default();
        let a = r(Value::Int(2));
        let b = Value::Int(3);
        assert_eq!(add(&a, &b, &mut d), Ok(Value::Int(5)));
        assert_eq!(
            loose_equals(&a, &Value::Int(2), &mut d),
            Ok(Value::Bool(true))
        );
        assert_eq!(concat(&a, &b, &mut d), Ok(Value::String("23".into())));
        assert_eq!(increment(&a, &mut d), Ok(Value::Int(3)));
        assert_eq!(compare(&a, &b, &mut d), Ok(-1));
        assert_eq!(d.entries.len(), 0);
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        // 50_000 nested arrays, compare and identity through them.
        let mut d = CollectDiagnostics::default();
        let inner = Value::Array(std::rc::Rc::new(PhpArray::new()));
        let mut v = inner.clone();
        for _ in 0..50_000 {
            let mut a = PhpArray::new();
            a.push(v).unwrap();
            v = Value::Array(std::rc::Rc::new(a));
        }
        let w = v.clone();
        assert_eq!(compare(&v, &w, &mut d), Ok(0));
        assert_eq!(strict_equals(&v, &w, &mut d), Ok(Value::Bool(true)));
    }

    #[test]
    fn no_diagnostics_ignores_and_collect_records() {
        let mut n = NoDiagnostics;
        n.warning("x".into());
        let mut c = CollectDiagnostics::default();
        c.warning("w".into());
        c.deprecated("d".into());
        c.notice("n".into());
        assert_eq!(c.messages(), vec!["w", "d", "n"]);
    }
}
