//! Arithmetic: `+`, `-`, `*`, `/`, `%`, `**`, unary `-` and `+`.

use crate::array::PhpArray;
use crate::convert::NumVal;
use crate::error::{Diagnostics, ErrorKind, OpError};
use crate::ops::{deref_clone, signed_mul, to_number2, try_get_long2};
use crate::value::Value;

/// Adds two arrays PHP-style: the left operand wins on key conflicts, new
/// right-hand keys are appended in order.
fn array_union(a: &PhpArray, b: &PhpArray) -> Value {
    let mut out = a.clone();
    for (k, v) in b.iter() {
        if !out.contains_key(k) {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Array(std::rc::Rc::new(out))
}

/// `zend_operators.c`'s conversion-then-retry: one of the pair converts, so
/// re-run the numeric cases on the converted operands.
fn fast(a: &Value, b: &Value, sigil: &str, diag: &mut dyn Diagnostics) -> Result<NumVal2, OpError> {
    let (na, nb) = to_number2(a, b, sigil, diag)?;
    Ok(match (na, nb) {
        (NumVal::Int(x), NumVal::Int(y)) => NumVal2::II(x, y),
        (NumVal::Int(x), NumVal::Float(y)) => NumVal2::FF(x as f64, y),
        (NumVal::Float(x), NumVal::Int(y)) => NumVal2::FF(x, y as f64),
        (NumVal::Float(x), NumVal::Float(y)) => NumVal2::FF(x, y),
    })
}

enum NumVal2 {
    II(i64, i64),
    FF(f64, f64),
}

fn add_fast(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => match x.checked_add(*y) {
            Some(v) => Ok(Value::Int(v)),
            None => Ok(Value::Float(*x as f64 + *y as f64)),
        },
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(*x as f64 + y)),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x + *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
        (Value::Array(x), Value::Array(y)) => Ok(array_union(x, y)),
        _ => match fast(a, b, sigil, diag)? {
            NumVal2::II(x, y) => match x.checked_add(y) {
                Some(v) => Ok(Value::Int(v)),
                None => Ok(Value::Float(x as f64 + y as f64)),
            },
            NumVal2::FF(x, y) => Ok(Value::Float(x + y)),
        },
    }
}

fn sub_fast(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => match x.checked_sub(*y) {
            Some(v) => Ok(Value::Int(v)),
            None => Ok(Value::Float(*x as f64 - *y as f64)),
        },
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(*x as f64 - y)),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x - *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
        _ => match fast(a, b, sigil, diag)? {
            NumVal2::II(x, y) => match x.checked_sub(y) {
                Some(v) => Ok(Value::Int(v)),
                None => Ok(Value::Float(x as f64 - y as f64)),
            },
            NumVal2::FF(x, y) => Ok(Value::Float(x - y)),
        },
    }
}

fn mul_fast(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            let (p, d, ov) = signed_mul(*x, *y);
            Ok(if ov { Value::Float(d) } else { Value::Int(p) })
        }
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(*x as f64 * y)),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x * *y as f64)),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
        _ => match fast(a, b, sigil, diag)? {
            NumVal2::II(x, y) => {
                let (p, d, ov) = signed_mul(x, y);
                Ok(if ov { Value::Float(d) } else { Value::Int(p) })
            }
            NumVal2::FF(x, y) => Ok(Value::Float(x * y)),
        },
    }
}

fn div_fast(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    let dbz = || OpError::new(ErrorKind::DivisionByZeroError, "Division by zero");
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                return Err(dbz());
            }
            if *y == -1 && *x == i64::MIN {
                return Ok(Value::Float(i64::MIN as f64 / -1.0));
            }
            Ok(if x % y == 0 {
                Value::Int(x / y)
            } else {
                Value::Float(*x as f64 / *y as f64)
            })
        }
        (Value::Int(x), Value::Float(y)) => {
            if *y == 0.0 {
                return Err(dbz());
            }
            Ok(Value::Float(*x as f64 / y))
        }
        (Value::Float(x), Value::Int(y)) => {
            if *y == 0 {
                return Err(dbz());
            }
            Ok(Value::Float(x / *y as f64))
        }
        (Value::Float(x), Value::Float(y)) => {
            if *y == 0.0 {
                return Err(dbz());
            }
            Ok(Value::Float(x / y))
        }
        _ => {
            let (na, nb) = to_number2(a, b, sigil, diag)?;
            let fa = |n: NumVal| match n {
                NumVal::Int(i) => Value::Int(i),
                NumVal::Float(f) => Value::Float(f),
            };
            div_fast(&fa(na), &fa(nb), sigil, diag)
        }
    }
}

fn pow_fast(
    a: &Value,
    b: &Value,
    sigil: &str,
    diag: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    // `safe_pow` plus the deprecation PHP emits for a zero base.
    let safe_pow = |base: f64, exp: f64, diag: &mut dyn Diagnostics| -> f64 {
        if base == 0.0 && exp < 0.0 {
            diag.deprecated("Power of base 0 and negative exponent is deprecated".to_string());
        }
        base.powf(exp)
    };
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y < 0 {
                return Ok(Value::Float(safe_pow(*x as f64, *y as f64, diag)));
            }
            if *y == 0 {
                return Ok(Value::Int(1));
            }
            if *x == 0 {
                return Ok(Value::Int(0));
            }
            // Square-and-multiply exactly like pow_function_base.
            let mut l1: i64 = 1;
            let mut l2: i64 = *x;
            let mut i: i64 = *y;
            while i >= 1 {
                if i % 2 != 0 {
                    i -= 1;
                    let (p, d, ov) = signed_mul(l1, l2);
                    if ov {
                        return Ok(Value::Float(d * safe_pow(l2 as f64, i as f64, diag)));
                    }
                    l1 = p;
                } else {
                    i /= 2;
                    let (p2, d, ov) = signed_mul(l2, l2);
                    if ov {
                        return Ok(Value::Float(l1 as f64 * safe_pow(d, i as f64, diag)));
                    }
                    l2 = p2;
                }
            }
            Ok(Value::Int(l1))
        }
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(safe_pow(*x as f64, *y, diag))),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(safe_pow(*x, *y as f64, diag))),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(safe_pow(*x, *y, diag))),
        _ => {
            let (na, nb) = to_number2(a, b, sigil, diag)?;
            let fa = |n: NumVal| match n {
                NumVal::Int(i) => Value::Int(i),
                NumVal::Float(f) => Value::Float(f),
            };
            pow_fast(&fa(na), &fa(nb), sigil, diag)
        }
    }
}

/// `%`: both operands go through `zendi_try_get_long`.
pub fn modulo(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let sigil = "%";
    let a = deref_clone(a);
    let b = deref_clone(b);
    let (x, y) = try_get_long2(&a, &b, sigil, d)?;
    if y == 0 {
        return Err(OpError::new(
            ErrorKind::DivisionByZeroError,
            "Modulo by zero",
        ));
    }
    if y == -1 {
        return Ok(Value::Int(0)); // avoids i64::MIN % -1 overflow
    }
    Ok(Value::Int(x.wrapping_rem(y)))
}

pub fn add(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    add_fast(&a, &b, "+", d)
}

pub fn sub(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    sub_fast(&a, &b, "-", d)
}

pub fn mul(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    mul_fast(&a, &b, "*", d)
}

pub fn div(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    div_fast(&a, &b, "/", d)
}

pub fn pow(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    pow_fast(&a, &b, "**", d)
}

fn unop(v: &Value, factor: i64, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let one = Value::Int(factor);
    mul_fast(&deref_clone(v), &one, "*", d)
}

/// Unary minus. PHP 8.5 lowers it to a multiply, and its TypeError says so.
pub fn negate(v: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    unop(v, -1, d)
}

/// Unary plus: a no-op for numbers, otherwise the same conversion error PHP
/// reports (a multiply by one).
pub fn unary_plus(v: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    unop(v, 1, d)
}
