//! Tests of `PhpArray`: what PHP code can observe (`contracts`), key
//! normalisation (`keys`), the two storage shapes (`storage`), and a long
//! random walk checked against a plain list of pairs (`model`).
//!
//! These modules sit inside `array`, so they may look at private fields such
//! as `storage` and `dead`; the shape an array has is invisible from outside.

mod contracts;
mod keys;
mod model;
mod storage;

use super::ArrayKey;
use crate::value::Value;

/// An integer key.
fn k(i: i64) -> ArrayKey {
    ArrayKey::Int(i)
}

/// The key `$a[$s]` uses: `"5"` becomes the integer 5, `"05"` stays a string.
fn ks(s: &str) -> ArrayKey {
    ArrayKey::from_value(&Value::from(s)).unwrap()
}

/// An integer value.
fn vi(i: i64) -> Value {
    Value::Int(i)
}
