//! Natural ordering, adapted from Martin Pool's strnatcmp algorithm in PHP.
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
use std::cmp::Ordering;

const PARAMETERS: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("strnatcmp", PARAMETERS, sensitive));
    registry.function(FunctionEntry::sync(
        "strnatcasecmp",
        PARAMETERS,
        insensitive,
    ));
}
fn sensitive(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(
        natural_compare(args::bytes(arguments, 0), args::bytes(arguments, 1), false) as i64,
    ))
}
fn insensitive(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
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
    while left.first() == Some(&b'0') && left.get(1).is_some_and(u8::is_ascii_digit) {
        left = &left[1..];
    }
    while right.first() == Some(&b'0') && right.get(1).is_some_and(u8::is_ascii_digit) {
        right = &right[1..];
    }
    loop {
        while left
            .first()
            .is_some_and(|b| b.is_ascii_whitespace() || *b == 11)
        {
            left = &left[1..];
        }
        while right
            .first()
            .is_some_and(|b| b.is_ascii_whitespace() || *b == 11)
        {
            right = &right[1..];
        }
        if left.first().is_some_and(u8::is_ascii_digit)
            && right.first().is_some_and(u8::is_ascii_digit)
        {
            let fractional = left.first() == Some(&b'0') || right.first() == Some(&b'0');
            let left_digits = left.iter().take_while(|b| b.is_ascii_digit()).count();
            let right_digits = right.iter().take_while(|b| b.is_ascii_digit()).count();
            let order = if fractional {
                left[..left_digits].cmp(&right[..right_digits])
            } else {
                left_digits
                    .cmp(&right_digits)
                    .then_with(|| left[..left_digits].cmp(&right[..right_digits]))
            };
            if order != Ordering::Equal {
                return order;
            }
            left = &left[left_digits..];
            right = &right[right_digits..];
            if left.is_empty() || right.is_empty() {
                return left.len().cmp(&right.len());
            }
        }
        let a = left.first().copied().unwrap_or(0);
        let b = right.first().copied().unwrap_or(0);
        let order = if fold_case {
            a.to_ascii_uppercase().cmp(&b.to_ascii_uppercase())
        } else {
            a.cmp(&b)
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
