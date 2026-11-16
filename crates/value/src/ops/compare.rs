//! Comparison: `==`, `===`, `<=>`, `<`, `<=`.
//!
//! Port of `zend_compare` / `zend_is_identical` (Zend/zend_operators.c).
//! Nested arrays are flattened through an explicit stack so deep structures
//! cannot overflow the Rust stack.

use std::rc::Rc;

use crate::array::{ArrayKey, PhpArray};
use crate::convert::{NumVal, parse_numeric};
use crate::error::{Diagnostics, OpError};
use crate::ops::deref_clone;
use crate::value::Value;

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
pub fn strict_equals(a: &Value, b: &Value, _d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    Ok(Value::Bool(identical(a, b)))
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
    let a = deref_clone(a);
    let b = deref_clone(b);
    compare_resolved(&a, &b, d)
}

fn compare_resolved(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<i8, OpError> {
    // Resources compare numerically as their ids (zend's silent number
    // conversion); both-resource pairs become id vs id.
    if a.is_resource() || b.is_resource() {
        let ra = match a {
            Value::Resource(r) => Value::Int(r.id),
            other => silent_number(other),
        };
        let rb = match b {
            Value::Resource(r) => Value::Int(r.id),
            other => silent_number(other),
        };
        return compare_resolved(&ra, &rb, d);
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(cmp3(x.cmp(y))),
        (Value::Float(x), Value::Int(y)) => Ok(threeway(*x, *y as f64)),
        (Value::Int(x), Value::Float(y)) => Ok(threeway(*x as f64, *y)),
        (Value::Float(x), Value::Float(y)) => Ok(threeway(*x, *y)),
        (Value::Array(x), Value::Array(y)) => {
            if Rc::ptr_eq(x, y) {
                Ok(0)
            } else {
                compare_arrays(x, y, d)
            }
        }
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
            if other.is_object() {
                // Same instance, or property comparison within one class;
                // PHP reports different classes as 1 (uncomparable) from
                // either side.
                if let (Value::Object(x), Value::Object(y)) = (obj, other) {
                    if Rc::ptr_eq(x, y) {
                        return Ok(0);
                    }
                    if x.class.name() != y.class.name() {
                        return Ok(1);
                    }
                    return compare_arrays(&x.to_array(), &y.to_array(), d);
                }
            }
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
            match other {
                Value::Int(_) | Value::Float(_) => {
                    // stdClass cannot cast to numbers: PHP notices and
                    // treats the object as 1.
                    d.notice(format!(
                        "Object of class {} could not be converted to {}",
                        obj.type_name(),
                        if other.is_int() { "int" } else { "float" }
                    ));
                    let one = if other.is_int() {
                        Value::Int(1)
                    } else {
                        Value::Float(1.0)
                    };
                    if obj_first {
                        compare_resolved(&one, other, d)
                    } else {
                        compare_resolved(other, &one, d)
                    }
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
fn threeway(x: f64, y: f64) -> i8 {
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
fn smart_strcmp(s1: &[u8], s2: &[u8]) -> i8 {
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
        _ => cmp3(l.to_string().as_bytes().cmp(s)),
    }
}

/// `compare_double_to_string` (the NaN case returns early in `compare`).
fn double_to_string_cmp(d: f64, s: &[u8]) -> i8 {
    let p = parse_numeric(s);
    match p.value {
        Some(NumVal::Int(sl)) if p.exact => threeway(d, sl as f64),
        Some(NumVal::Float(sd)) if p.exact => threeway(d, sd),
        _ => cmp3(
            crate::float_format::float_to_string_precision14(d)
                .as_bytes()
                .cmp(s),
        ),
    }
}

// ---------------------------------------------------------------------------
// arrays: loose compare (`zend_hash_compare`, unordered), flattened
// iteratively so nesting depth cannot exhaust the stack
// ---------------------------------------------------------------------------

/// One nested-array frame: compare `a` against `b` from position `pos`.
struct Frame<'x> {
    a: &'x PhpArray,
    b: &'x PhpArray,
    pos: Option<usize>,
}

/// `zend_hash_compare` with `ordered = false`: same element count required,
/// then every key of the left array must exist on the right (else right is
/// smaller) and values compare loosely; first difference decides.
fn compare_arrays(a: &PhpArray, b: &PhpArray, d: &mut dyn Diagnostics) -> Result<i8, OpError> {
    if a.len() != b.len() {
        return Ok(if a.len() > b.len() { 1 } else { -1 });
    }
    let mut stack = vec![Frame {
        a,
        b,
        pos: a.first_position(),
    }];
    while let Some(mut cur) = stack.pop() {
        while let Some(pos) = cur.pos {
            let (k, v) = cur.a.entry_at(pos).unwrap();
            let bv = match cur.b.get(k) {
                Some(bv) => bv,
                None => return Ok(1),
            };
            cur.pos = cur.a.next_position(pos);
            if let (Value::Array(x), Value::Array(y)) = (v, bv) {
                stack.push(cur);
                stack.push(Frame {
                    a: x,
                    b: y,
                    pos: x.first_position(),
                });
                // The subtree runs next; its non-zero results return
                // directly, exhausting it means 0 and this frame resumes.
                break;
            }
            let r = compare_resolved(v, bv, d)?;
            if r != 0 {
                return Ok(r);
            }
        }
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// strict identity (`===`)
// ---------------------------------------------------------------------------

/// `zend_is_identical` (after dereferencing). Float identity is IEEE `==`,
/// so `NAN === NAN` is false; objects and resources are themselves only.
fn identical(a: &Value, b: &Value) -> bool {
    let x = &deref_clone(a);
    let y = &deref_clone(b);
    match (x, y) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => identical_arrays(x, y),
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        (Value::Resource(x), Value::Resource(y)) => Rc::ptr_eq(x, y),
        (Value::Reference(_), _) | (_, Value::Reference(_)) => {
            // Nested references do not occur, but stay safe: compare owned.
            let x = deref_clone(a);
            let y = deref_clone(b);
            identical(&x, &y)
        }
        _ => false,
    }
}

/// One ordered-array frame: `a`'s entry at `apos` against `b`'s at `bpos`.
struct ZipFrame<'x> {
    a: &'x PhpArray,
    b: &'x PhpArray,
    apos: Option<usize>,
    bpos: Option<usize>,
}

/// `zend_hash_compare` with `ordered = true` and an identity checker: same
/// sizes, keys pairwise in order (int vs int by value, string vs string by
/// bytes; mixed key types never match), values identical.
fn identical_arrays(a: &PhpArray, b: &PhpArray) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut stack = vec![ZipFrame {
        a,
        b,
        apos: a.first_position(),
        bpos: b.first_position(),
    }];
    while let Some(mut cur) = stack.pop() {
        while let (Some(ap), Some(bp)) = (cur.apos, cur.bpos) {
            let (ka, va) = cur.a.entry_at(ap).unwrap();
            let (kb, vb) = cur.b.entry_at(bp).unwrap();
            let keys_match = match (ka, kb) {
                (ArrayKey::Int(x), ArrayKey::Int(y)) => x == y,
                (ArrayKey::String(x), ArrayKey::String(y)) => x == y,
                _ => false,
            };
            if !keys_match {
                return false;
            }
            cur.apos = cur.a.next_position(ap);
            cur.bpos = cur.b.next_position(bp);
            if let (Value::Array(x), Value::Array(y)) = (va, vb) {
                stack.push(cur);
                stack.push(ZipFrame {
                    a: x,
                    b: y,
                    apos: x.first_position(),
                    bpos: y.first_position(),
                });
                break;
            }
            if !identical(va, vb) {
                return false;
            }
        }
    }
    true
}
