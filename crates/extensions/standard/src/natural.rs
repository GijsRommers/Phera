//! Safe Rust port of php-src ext/standard/strnatcmp.c: strnatcmp_ex.
//! Digit runs retain Martin Pool's magnitude and leading-zero rules.

use std::cmp::Ordering;

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};
/*

  Modified for PHP by Andrei Zmievski <andrei@ispi.net>

  strnatcmp.c -- Perform 'natural order' comparisons of strings in C.
  Copyright (C) 2000 by Martin Pool <mbp@humbug.org.au>

  This software is provided 'as-is', without any express or implied
  warranty.  In no event will the authors be held liable for any damages
  arising from the use of this software.

  Permission is granted to anyone to use this software for any purpose,
  including commercial applications, and to alter it and redistribute it
  freely, subject to the following restrictions:

  1. The origin of this software must not be misrepresented; you must not
     claim that you wrote the original software. If you use this software
     in a product, an acknowledgment in the product documentation would be
     appreciated but is not required.
  2. Altered source versions must be plainly marked as such, and must not be
     misrepresented as being the original software.
  3. This notice may not be removed or altered from any source distribution.
*/
// Altered version: safe Rust slices replace C pointers; no locale-dependent folding.

const NATURAL_COMPARE_PARAMS: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "strnatcmp",
        NATURAL_COMPARE_PARAMS,
        strnatcmp,
    ));
    registry.function(FunctionEntry::sync(
        "strnatcasecmp",
        NATURAL_COMPARE_PARAMS,
        strnatcasecmp,
    ));
}

fn strnatcmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(
        natural_compare(args::bytes(arguments, 0), args::bytes(arguments, 1), false) as i64,
    ))
}

fn strnatcasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(
        natural_compare(args::bytes(arguments, 0), args::bytes(arguments, 1), true) as i64,
    ))
}

/// Leading zeros are ignored only at the start; embedded zero-prefixed runs
/// compare as fractions, while other digit runs compare by magnitude.
pub(crate) fn natural_compare(mut left: &[u8], mut right: &[u8], fold_case: bool) -> Ordering {
    if left.is_empty() || right.is_empty() {
        return left.len().cmp(&right.len());
    }
    left = skip_leading_zeros(left);
    right = skip_leading_zeros(right);
    loop {
        left = skip_whitespace(left);
        right = skip_whitespace(right);
        if left.first().is_some_and(u8::is_ascii_digit)
            && right.first().is_some_and(u8::is_ascii_digit)
        {
            let order = compare_digit_runs(&mut left, &mut right);
            if order != Ordering::Equal {
                return order;
            }
            if left.is_empty() || right.is_empty() {
                return left.len().cmp(&right.len());
            }
        }
        // The C routine compares its terminating NUL with embedded NUL bytes too.
        let left_byte = left.first().copied().unwrap_or(0);
        let right_byte = right.first().copied().unwrap_or(0);
        let order = if fold_case {
            left_byte
                .to_ascii_uppercase()
                .cmp(&right_byte.to_ascii_uppercase())
        } else {
            left_byte.cmp(&right_byte)
        };
        if order != Ordering::Equal {
            return order;
        }
        left = left.get(1..).unwrap_or_default();
        right = right.get(1..).unwrap_or_default();
        if left.is_empty() || right.is_empty() {
            return left.len().cmp(&right.len());
        }
    }
}

/// Only initial zeros disappear; zeros inside a string select fractional comparison.
fn skip_leading_zeros(mut bytes: &[u8]) -> &[u8] {
    while let [b'0', next, rest @ ..] = bytes {
        if !next.is_ascii_digit() {
            break;
        }
        bytes = bytes.get(1..).unwrap_or(rest);
    }
    bytes
}

/// PHP's C-locale whitespace includes vertical tab.
fn skip_whitespace(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if !first.is_ascii_whitespace() && *first != 11 {
            break;
        }
        bytes = rest;
    }
    bytes
}

/// Zero-prefixed runs compare lexically; other runs compare magnitude first.
fn compare_digit_runs(left: &mut &[u8], right: &mut &[u8]) -> Ordering {
    let fractional = left.first() == Some(&b'0') || right.first() == Some(&b'0');
    let left_length = left.iter().take_while(|byte| byte.is_ascii_digit()).count();
    let right_length = right
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let left_digits = left.get(..left_length).unwrap_or_default();
    let right_digits = right.get(..right_length).unwrap_or_default();
    let order = if fractional {
        left_digits.cmp(right_digits)
    } else {
        left_length
            .cmp(&right_length)
            .then_with(|| left_digits.cmp(right_digits))
    };
    *left = left.get(left_length..).unwrap_or_default();
    *right = right.get(right_length..).unwrap_or_default();
    order
}
