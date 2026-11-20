//! Comparison: `==`, `===`, `<=>`, `<`, `<=`.
//!
//! Port of `zend_compare` / `zend_is_identical` (Zend/zend_operators.c).
//!
//! ```php
//! $a = [1]; $a[] = &$a;
//! $b = [1]; $b[] = &$b;
//! $a === $a; // true
//! $a == $b;  // Error: Nesting level too deep - recursive dependency?
//! ```

use std::rc::Rc;

use crate::array::PhpArray;
use crate::convert::{NumVal, parse_numeric};
use crate::error::{Diagnostics, OpError};
use crate::float_format::{NumberText, float_text_precision14};
use crate::object::{Object, ObjectCast};
use crate::value::Value;

mod casting;
mod structure;
pub use casting::{
    LazyComparison, StringComparison, StringComparisonStep, compare_with_string_objects,
};
use structure::{Comparison, Table, compare_tables};

/// `<=>` as an operator: -1, 0 or 1 in a `Value::Int`.
pub fn compare_op(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    Ok(Value::Int(i64::from(compare(a, b, d)?)))
}

/// `==` (loose equality); PHP's `==` is `zend_compare(...) == 0`.
pub fn loose_equals(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    Ok(Value::Bool(compare(a, b, d)? == 0))
}

/// `===` (strict identity). Arrays must match in order, key types and value
/// types; objects and resources only equal themselves.
pub fn strict_equals(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = a.try_deref()?;
    let b = b.try_deref()?;
    let equal = match (&*a, &*b) {
        (Value::Array(a), Value::Array(b)) if !Rc::ptr_eq(a, b) => {
            compare_tables(
                Table::Array(a.clone()),
                Table::Array(b.clone()),
                &mut Comparison::Strict,
                d,
            )? == 0
        }
        _ => identical_leaf(&a, &b),
    };
    Ok(Value::Bool(equal))
}

pub fn less(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    Ok(Value::Bool(compare(a, b, d)? < 0))
}

pub fn less_equal(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    Ok(Value::Bool(compare(a, b, d)? <= 0))
}

/// `zend_compare`'s three-way compare. Returns -1/0/1.
///
/// PHP quirks kept deliberately: NaN compares as 1 (even against itself in a
/// float pair), and different-class objects compare as 1 in both directions
/// (`ZEND_UNCOMPARABLE == 1`).
pub fn compare(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<i8, OpError> {
    let a = a.try_deref()?;
    let b = b.try_deref()?;
    compare_resolved(&a, &b, &mut Comparison::Loose, d)
}

/// A native sort keeps comparing while the first comparison error is pending.
pub fn compare_after_error(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<i8, OpError> {
    let a = a.try_deref()?;
    let b = b.try_deref()?;
    compare_resolved(&a, &b, &mut Comparison::PendingError, d)
}

/// A native class can supply an ordering or the two property tables to compare.
pub enum ObjectComparison {
    InitializeLazy,
    Default,
    Compared(i8),
    Projected(PhpArray, PhpArray),
}

/// Operands offered to the runtime's comparison hooks.
pub enum ObjectOperands<'a> {
    Pair(&'a Object, &'a Object),
    Mixed(&'a Value, &'a Value),
}

pub type ObjectComparator<'a> =
    dyn FnMut(ObjectOperands<'_>, &mut dyn Diagnostics) -> Result<ObjectComparison, OpError> + 'a;

/// The same hook serves ordering and equality, including objects inside containers.
pub fn compare_with_objects(
    a: &Value,
    b: &Value,
    d: &mut dyn Diagnostics,
    compare: &mut ObjectComparator<'_>,
) -> Result<i8, OpError> {
    let a = a.try_deref()?;
    let b = b.try_deref()?;
    compare_resolved(&a, &b, &mut Comparison::Native(compare), d)
}

pub fn loose_equals_with_objects(
    a: &Value,
    b: &Value,
    d: &mut dyn Diagnostics,
    compare: &mut ObjectComparator<'_>,
) -> Result<Value, OpError> {
    Ok(Value::Bool(compare_with_objects(a, b, d, compare)? == 0))
}

fn compare_resolved(
    a: &Value,
    b: &Value,
    comparison: &mut Comparison<'_, '_>,
    d: &mut dyn Diagnostics,
) -> Result<i8, OpError> {
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            if Rc::ptr_eq(a, b) {
                Ok(0)
            } else {
                compare_tables(
                    Table::Array(a.clone()),
                    Table::Array(b.clone()),
                    comparison,
                    d,
                )
            }
        }
        (Value::Object(a), Value::Object(b)) => match comparison.objects(a, b, d)? {
            structure::Pair::Compared(result) => Ok(result),
            structure::Pair::Initialize => Err(OpError::error(
                "Object comparison requires PHP initialization",
            )),
            structure::Pair::Tables(left, right) => compare_tables(left, right, comparison, d),
        },
        _ => match comparison.mixed(a, b, d)? {
            Some(order) => Ok(order),
            None => comparison.leaf(a, b, d),
        },
    }
}

fn compare_leaf(
    a: &Value,
    b: &Value,
    comparison: &Comparison<'_, '_>,
    d: &mut dyn Diagnostics,
) -> Result<i8, OpError> {
    // Resources compare numerically as their ids (zend's silent number
    // conversion); both-resource pairs become id vs id.
    if a.is_resource() || b.is_resource() {
        if matches!(comparison, Comparison::PendingError)
            && !matches!(a, Value::Null | Value::Bool(_) | Value::Object(_))
            && !matches!(b, Value::Null | Value::Bool(_) | Value::Object(_))
        {
            return Ok(1);
        }
        let ra = match a {
            Value::Resource(r) => Value::Int(r.id),
            other => silent_number(other),
        };
        let rb = match b {
            Value::Resource(r) => Value::Int(r.id),
            other => silent_number(other),
        };
        return compare_non_resource(&ra, &rb, comparison, d);
    }
    compare_non_resource(a, b, comparison, d)
}

fn compare_non_resource(
    a: &Value,
    b: &Value,
    comparison: &Comparison<'_, '_>,
    d: &mut dyn Diagnostics,
) -> Result<i8, OpError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(cmp3(x.cmp(y))),
        (Value::Float(x), Value::Int(y)) => Ok(threeway(*x, *y as f64)),
        (Value::Int(x), Value::Float(y)) => Ok(threeway(*x as f64, *y)),
        (Value::Float(x), Value::Float(y)) => Ok(threeway(*x, *y)),
        (Value::Null, Value::Null | Value::Bool(false))
        | (Value::Bool(false), Value::Null | Value::Bool(false))
        | (Value::Bool(true), Value::Bool(true)) => Ok(0),
        (Value::Null, Value::Bool(true)) => Ok(-1),
        (Value::Bool(true), Value::Null) => Ok(1),
        (Value::String(x), Value::String(y)) => Ok(smart_strcmp(x.as_bytes(), y.as_bytes())),
        (Value::Null, Value::String(y)) => Ok(if y.is_empty() { 0 } else { -1 }),
        (Value::String(x), Value::Null) => Ok(if x.is_empty() { 0 } else { 1 }),
        (Value::Int(x), Value::String(y)) => Ok(long_to_string_cmp(*x, y.as_bytes())),
        (Value::String(x), Value::Int(y)) => Ok(-long_to_string_cmp(*y, x.as_bytes())),
        (Value::Float(x), Value::String(y)) => {
            if x.is_nan() {
                return Ok(1);
            }
            Ok(double_to_string_cmp(*x, y.as_bytes()))
        }
        (Value::String(x), Value::Float(y)) => {
            if y.is_nan() {
                return Ok(1);
            }
            Ok(-double_to_string_cmp(*y, x.as_bytes()))
        }
        (Value::Object(_), Value::Null) => Ok(1),
        (Value::Null, Value::Object(_)) => Ok(-1),
        _ if a.is_object() || b.is_object() => {
            let (obj, other, obj_first) = if a.is_object() {
                (a, b, true)
            } else {
                (b, a, false)
            };
            if other.is_bool() {
                // Bool comparison, order-sensitive like zend_compare: a
                // falsy left operand sorts below, a `true` left operand
                // sorts equal-or-above.
                let ot = truthy(obj);
                let bt = truthy(other);
                return Ok(match (obj_first, ot, bt) {
                    // op1 is the bool.
                    (false, _, false) => {
                        if ot {
                            -1
                        } else {
                            0
                        }
                    }
                    (false, _, true) => {
                        if ot {
                            0
                        } else {
                            1
                        }
                    }
                    // op1 is the object.
                    (true, _, false) => {
                        if ot {
                            1
                        } else {
                            0
                        }
                    }
                    (true, _, true) => {
                        if ot {
                            0
                        } else {
                            -1
                        }
                    }
                });
            }
            if matches!(obj, Value::Object(object) if object.class.is_enum()) {
                return Ok(1);
            }
            match other {
                Value::Int(_) | Value::Float(_) => {
                    if let Value::Object(object) = obj {
                        let kind = if other.is_int() {
                            ObjectCast::Int
                        } else {
                            ObjectCast::Float
                        };
                        match (object.class.cast(object, kind), other) {
                            (Some(Value::Int(left)), Value::Int(right)) => {
                                return Ok(cmp3(if obj_first {
                                    left.cmp(right)
                                } else {
                                    right.cmp(&left)
                                }));
                            }
                            (Some(Value::Float(left)), Value::Float(right)) => {
                                return Ok(if obj_first {
                                    threeway(left, *right)
                                } else {
                                    threeway(*right, left)
                                });
                            }
                            _ => {}
                        }
                    }
                    // stdClass cannot cast to numbers: PHP notices and
                    // treats the object as 1.
                    d.notice(format!(
                        "Object of class {} could not be converted to {}",
                        obj.type_name(),
                        if other.is_int() { "int" } else { "float" }
                    ));
                    Ok(match (obj_first, other) {
                        (true, Value::Int(other)) => cmp3(1.cmp(other)),
                        (false, Value::Int(other)) => cmp3(other.cmp(&1)),
                        (true, Value::Float(other)) => threeway(1.0, *other),
                        (false, Value::Float(other)) => threeway(*other, 1.0),
                        _ => 1,
                    })
                }
                // Uncastable targets: the object side sorts greater.
                _ => Ok(if obj_first { 1 } else { -1 }),
            }
        }
        // Remaining pairs: bool rules first (truthiness of both sides),
        // then arrays sort greater than any non-array. NaN against
        // null/bool lands here too and compares by truthiness.
        _ => {
            if falsy(a) {
                return Ok(if truthy(b) { -1 } else { 0 });
            }
            if a.is_bool() {
                return Ok(if truthy(b) { 0 } else { 1 });
            }
            if falsy(b) {
                return Ok(if truthy(a) { 1 } else { 0 });
            }
            if b.is_bool() {
                return Ok(if truthy(a) { 0 } else { -1 });
            }
            if matches!(comparison, Comparison::PendingError) {
                // Zend stops after its generic numeric conversion when an error is pending.
                return Ok(1);
            }
            if a.is_array() {
                return Ok(1);
            }
            if b.is_array() {
                return Ok(-1);
            }
            Ok(1)
        }
    }
}

/// `_zendi_convert_scalar_to_number_silent`: no warnings, non-numeric
/// strings become 0.
fn silent_number(v: &Value) -> Value {
    match v {
        Value::Int(_) | Value::Float(_) => v.clone(),
        Value::Null | Value::Bool(false) => Value::Int(0),
        Value::Bool(true) => Value::Int(1),
        Value::String(s) => match parse_numeric(s.as_bytes()).value {
            Some(NumVal::Int(i)) => Value::Int(i),
            Some(NumVal::Float(f)) => Value::Float(f),
            None => Value::Int(0),
        },
        other => other.clone(),
    }
}

fn falsy(v: &Value) -> bool {
    matches!(v, Value::Null | Value::Bool(false))
}

fn truthy(v: &Value) -> bool {
    crate::convert::to_bool(v)
}

fn cmp3(o: std::cmp::Ordering) -> i8 {
    match o {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `ZEND_THREEWAY_COMPARE`: `a == b ? 0 : a < b ? -1 : 1`; NaN therefore
/// compares as greater.
pub fn threeway(x: f64, y: f64) -> i8 {
    if x == y {
        0
    } else if x < y {
        -1
    } else {
        1
    }
}

/// `zendi_smart_strcmp`: numeric comparison when both strings are numeric
/// (no trailing data), byte comparison otherwise.
pub fn smart_strcmp(s1: &[u8], s2: &[u8]) -> i8 {
    let p1 = parse_numeric(s1);
    let p2 = parse_numeric(s2);
    match (p1.value, p2.value) {
        (Some(v1), Some(v2)) if p1.exact && p2.exact => match (v1, v2) {
            (NumVal::Int(l1), NumVal::Int(l2)) => cmp3(l1.cmp(&l2)),
            (NumVal::Float(d1), NumVal::Int(l2)) => {
                // oflow belongs to the double side (an integer literal
                // beyond i64, which rounds to a double).
                if p1.oflow != 0 {
                    p1.oflow
                } else {
                    threeway(d1, l2 as f64)
                }
            }
            (NumVal::Int(l1), NumVal::Float(d2)) => {
                if p2.oflow != 0 {
                    -p2.oflow
                } else {
                    threeway(l1 as f64, d2)
                }
            }
            (NumVal::Float(d1), NumVal::Float(d2)) => {
                if p1.oflow != 0 && p1.oflow == p2.oflow && d1 == d2 {
                    // Both were integer literals overflowing to the same
                    // side: PHP falls back to a byte comparison.
                    cmp3(s1.cmp(s2))
                } else if d1 == d2 && !d1.is_finite() {
                    // Same infinity: byte comparison, like PHP.
                    cmp3(s1.cmp(s2))
                } else {
                    threeway(d1, d2)
                }
            }
        },
        _ => cmp3(s1.cmp(s2)),
    }
}

/// `compare_long_to_string`.
fn long_to_string_cmp(l: i64, s: &[u8]) -> i8 {
    let p = parse_numeric(s);
    match p.value {
        Some(NumVal::Int(sl)) if p.exact => cmp3(l.cmp(&sl)),
        Some(NumVal::Float(sd)) if p.exact => threeway(l as f64, sd),
        _ => cmp3(NumberText::integer(l).as_bytes().cmp(s)),
    }
}

/// `compare_double_to_string` (the NaN case returns early in `compare`).
fn double_to_string_cmp(d: f64, s: &[u8]) -> i8 {
    let p = parse_numeric(s);
    match p.value {
        Some(NumVal::Int(sl)) if p.exact => threeway(d, sl as f64),
        Some(NumVal::Float(sd)) if p.exact => threeway(d, sd),
        _ => cmp3(float_text_precision14(d).as_bytes().cmp(s)),
    }
}

/// PHP compares scalar types exactly, and objects and resources by identity.
fn identical_leaf(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
        (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
        (Value::Resource(a), Value::Resource(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
