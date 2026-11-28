//! Literal-array membership and pure synchronous built-ins executed inside the
//! interpreter loop.
//!
//! ```php
//! if (in_array($role, ['admin', 'editor'], true)) { ... }
//! if (str_starts_with($path, '/api/')) { ... }
//! ```
//!
//! When a `CallNativeDirect` symbol has already linked to a [`PureNative`]
//! built-in and its arguments match the exact non-coercing types (with no
//! warnings, errors, or callbacks possible), the loop computes the result
//! directly without leaving `Machine::run`. Any edge case returns `None`
//! before mutating any register so the VM's general handler runs it with full
//! PHP semantics.

use std::rc::Rc;

use phera_bytecode::{ConstId, Reg, SymbolId};
use phera_value::{NoDiagnostics, PhpArray, Value, power_of_two_digits};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::sync_native::PureNative;
use crate::fast::{ScalarStep, store};

const DEFAULT_TRIM_BYTES: &[u8] = b" \n\r\t\x0b\0";

impl Machine<'_> {
    /// `CallNativeDirect` of a pure built-in already linked in the unit cache.
    #[inline(never)]
    pub(super) fn call_native_direct<const GUARDED: bool>(
        &mut self,
        position: Position,
        symbol: SymbolId,
        (args, count): (Reg, u16),
        destination: Reg,
    ) -> Step {
        let cache = self.code.cache(position.frame.function);
        let Some(link) = cache
            .direct_natives
            .get()
            .and_then(|links| links.get(symbol.index()))
            .and_then(std::cell::Cell::get)
        else {
            return Step::Slow;
        };
        let Some(pure) = link.pure else {
            return Step::Slow;
        };
        let count = usize::from(count);
        if count < link.min || count > link.max {
            return Step::Slow;
        }
        let base = position.frame.base();
        let arg_base = base + args.index();
        let values = self.registers.window(arg_base, count);
        if values.len() != count {
            return Step::Slow;
        }
        let Some(result) = eval_pure_native(pure, values) else {
            return Step::Slow;
        };
        if self
            .registers
            .window(base + destination.index(), 1)
            .is_empty()
        {
            return Step::Slow;
        }
        // `CallNativeDirect` owns and clears its dedicated argument temporaries.
        for offset in 0..count {
            drop(self.registers.take_defined(arg_base + offset));
        }
        let slots = self.registers.tail_mut(base);
        match store::<GUARDED, _>(slots, destination, result) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `InLiteralArray`: `in_array($needle, ['a', 'b', ...], $strict)` in the loop.
    #[inline(never)]
    pub(super) fn in_literal_array<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (needle, haystack, strict): (Reg, ConstId, bool),
    ) -> Step {
        let base = position.frame.base();
        let found = {
            let Some(Value::Array(values)) = self
                .code
                .cache(position.frame.function)
                .constants
                .get(haystack.index())
            else {
                return Step::Slow;
            };
            let [Some(needle_slot)] = self.registers.window(base + needle.index(), 1) else {
                return Step::Slow;
            };
            let needle_val = needle_slot.deref();
            match &*needle_val {
                Value::InternedStr(needle) => values.values().any(|value| match value {
                    Value::InternedStr(other) => {
                        std::ptr::eq(*other, *needle) || other.as_bytes() == needle.as_bytes()
                    }
                    Value::String(other) => other.as_bytes() == needle.as_bytes(),
                    _ => false,
                }),
                Value::String(needle) => values.values().any(|value| {
                    value
                        .as_str_bytes()
                        .is_some_and(|other| other == needle.as_bytes())
                }),
                Value::Int(needle) if strict => values
                    .values()
                    .any(|value| matches!(value, Value::Int(value) if value == needle)),
                _ if strict => false,
                _ => return Step::Slow,
            }
        };
        let slots = self.registers.tail_mut(base);
        match store::<GUARDED, _>(slots, destination, Value::Bool(found)) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }
}

#[inline(always)]
fn as_str_bytes(slot: &Option<Value>) -> Option<&[u8]> {
    match slot.as_ref()? {
        Value::InternedStr(s) => Some(s.as_bytes()),
        Value::String(s) => Some(s.as_bytes()),
        _ => None,
    }
}

#[inline(always)]
fn as_int(slot: &Option<Value>) -> Option<i64> {
    match slot.as_ref()? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn eval_pure_native(pure: PureNative, values: &[Option<Value>]) -> Option<Value> {
    match pure {
        PureNative::StrStartsWith => {
            let [h, n] = values else { return None };
            Some(Value::Bool(as_str_bytes(h)?.starts_with(as_str_bytes(n)?)))
        }
        PureNative::StrEndsWith => {
            let [h, n] = values else { return None };
            Some(Value::Bool(as_str_bytes(h)?.ends_with(as_str_bytes(n)?)))
        }
        PureNative::StrContains => {
            let [h, n] = values else { return None };
            Some(Value::Bool(
                find_bytes(as_str_bytes(h)?, as_str_bytes(n)?).is_some(),
            ))
        }
        PureNative::Strpos => {
            let (haystack, needle, offset) = match values {
                [h, n] => (as_str_bytes(h)?, as_str_bytes(n)?, 0i64),
                [h, n, o] => (as_str_bytes(h)?, as_str_bytes(n)?, as_int(o)?),
                _ => return None,
            };
            let start = contained_offset(haystack.len(), offset)?;
            Some(match find_bytes(&haystack[start..], needle) {
                Some(pos) => Value::Int((start + pos) as i64),
                None => Value::Bool(false),
            })
        }
        PureNative::Strrpos => {
            let (haystack, needle, offset) = match values {
                [h, n] => (as_str_bytes(h)?, as_str_bytes(n)?, 0i64),
                [h, n, o] => (as_str_bytes(h)?, as_str_bytes(n)?, as_int(o)?),
                _ => return None,
            };
            let len = haystack.len();
            let pos = if offset >= 0 {
                let start = contained_offset(len, offset)?;
                rfind_bytes(&haystack[start..], needle).map(|p| start + p)
            } else {
                let limit = contained_offset(len, offset)?;
                if needle.is_empty() {
                    Some(limit)
                } else {
                    let end = limit.saturating_add(needle.len()).min(len);
                    rfind_bytes(&haystack[..end], needle)
                }
            };
            Some(match pos {
                Some(p) => Value::Int(p as i64),
                None => Value::Bool(false),
            })
        }
        PureNative::Substr => {
            let (string, offset, length) = match values {
                [s, o] => (as_str_bytes(s)?, as_int(o)?, None),
                [s, o, l] => {
                    let len = match l.as_ref()? {
                        Value::Null => None,
                        Value::Int(n) => Some(*n),
                        _ => return None,
                    };
                    (as_str_bytes(s)?, as_int(o)?, len)
                }
                _ => return None,
            };
            let range = clamped_range(string.len(), offset, length);
            Some(Value::from(&string[range]))
        }
        PureNative::Trim => eval_trim(values, true, true),
        PureNative::Ltrim => eval_trim(values, true, false),
        PureNative::Rtrim => eval_trim(values, false, true),
        PureNative::InArray => {
            let [
                Some(needle),
                Some(Value::Array(haystack)),
                Some(Value::Bool(true)),
            ] = values
            else {
                return None;
            };
            if needle.is_reference() {
                return None;
            }
            let found = match needle {
                Value::InternedStr(n) => haystack.values().any(|v| match &*v.deref() {
                    Value::InternedStr(o) => std::ptr::eq(*o, *n) || o.as_bytes() == n.as_bytes(),
                    Value::String(o) => o.as_bytes() == n.as_bytes(),
                    _ => false,
                }),
                Value::String(n) => haystack
                    .values()
                    .any(|v| v.deref().as_str_bytes() == Some(n.as_bytes())),
                Value::Int(n) => haystack
                    .values()
                    .any(|v| matches!(&*v.deref(), Value::Int(o) if o == n)),
                _ => haystack.values().any(|v| {
                    matches!(
                        phera_value::strict_equals(needle, &v.deref(), &mut NoDiagnostics),
                        Ok(Value::Bool(true))
                    )
                }),
            };
            Some(Value::Bool(found))
        }
        PureNative::ArrayKeyExists => {
            let [Some(key), Some(Value::Array(array))] = values else {
                return None;
            };
            let found = match key {
                Value::Int(k) => array.get_int(*k).is_some(),
                Value::Bool(k) => array.get_int(i64::from(*k)).is_some(),
                Value::InternedStr(k) => array.contains_interned(k),
                Value::String(k) => array.get_str(k).is_some(),
                _ => return None,
            };
            Some(Value::Bool(found))
        }
        PureNative::Implode => {
            let [glue, Some(Value::Array(array))] = values else {
                return None;
            };
            let glue = as_str_bytes(glue)?;
            if array.is_empty() {
                return Some(Value::from(""));
            }
            let mut out = Vec::new();
            for (idx, value) in array.values().enumerate() {
                if idx > 0 {
                    out.extend_from_slice(glue);
                }
                match &*value.deref() {
                    Value::InternedStr(s) => out.extend_from_slice(s.as_bytes()),
                    Value::String(s) => out.extend_from_slice(s.as_bytes()),
                    Value::Int(n) => {
                        use std::io::Write;
                        let _ = write!(out, "{n}");
                    }
                    Value::Bool(true) => out.push(b'1'),
                    Value::Bool(false) | Value::Null => {}
                    _ => return None,
                }
            }
            Some(Value::from(out))
        }
        PureNative::Explode => {
            let [sep, subject] = values else {
                return None;
            };
            let sep = as_str_bytes(sep)?;
            let subject = as_str_bytes(subject)?;
            if sep.is_empty() {
                return None;
            }
            let mut array = PhpArray::new();
            let mut rest = subject;
            while let Some(pos) = find_bytes(rest, sep) {
                array.push(Value::from(&rest[..pos])).ok()?;
                rest = &rest[pos + sep.len()..];
            }
            array.push(Value::from(rest)).ok()?;
            Some(Value::Array(Rc::new(array)))
        }
        PureNative::StrReplace => {
            let [search, replace, subject] = values else {
                return None;
            };
            let search_bytes = as_str_bytes(search)?;
            let replace_bytes = as_str_bytes(replace)?;
            let subject_bytes = as_str_bytes(subject)?;
            if search_bytes.is_empty() {
                return subject.clone();
            }
            let Some(first) = find_bytes(subject_bytes, search_bytes) else {
                return subject.clone();
            };
            let mut out = Vec::with_capacity(subject_bytes.len());
            out.extend_from_slice(&subject_bytes[..first]);
            out.extend_from_slice(replace_bytes);
            let mut rest = &subject_bytes[first + search_bytes.len()..];
            while let Some(pos) = find_bytes(rest, search_bytes) {
                out.extend_from_slice(&rest[..pos]);
                out.extend_from_slice(replace_bytes);
                rest = &rest[pos + search_bytes.len()..];
            }
            out.extend_from_slice(rest);
            Some(Value::from(out))
        }
        PureNative::Ord => {
            let [s] = values else { return None };
            let bytes = as_str_bytes(s)?;
            // PHP 8.5 emits a deprecation for empty or multi-byte strings.
            if bytes.len() != 1 {
                return None;
            }
            Some(Value::Int(i64::from(bytes[0])))
        }
        PureNative::Chr => {
            let [n] = values else { return None };
            let codepoint = as_int(n)?;
            // PHP 8.5 emits a deprecation outside 0..=255.
            if !(0..=255).contains(&codepoint) {
                return None;
            }
            Some(Value::from(vec![codepoint as u8]))
        }
        // An int argument only: a float or numeric string is coerced, with
        // possible deprecations, by the general handler.
        PureNative::Dechex | PureNative::Decbin | PureNative::Decoct => {
            let [n] = values else { return None };
            let bits = match pure {
                PureNative::Dechex => 4,
                PureNative::Decoct => 3,
                _ => 1,
            };
            Some(Value::String(power_of_two_digits(as_int(n)? as u64, bits)))
        }
        PureNative::Abs => {
            let [v] = values else { return None };
            match v.as_ref()? {
                Value::Int(n) => match n.checked_abs() {
                    Some(abs) => Some(Value::Int(abs)),
                    None => Some(Value::Float(-(*n as f64))),
                },
                Value::Float(f) => Some(Value::Float(f.abs())),
                _ => None,
            }
        }
        PureNative::ArrayKeys => {
            let [Some(Value::Array(array))] = values else {
                return None;
            };
            let keys = PhpArray::from_values(
                array.keys().map(|key| match key {
                    phera_value::ArrayKey::Int(i) => Value::Int(i),
                    phera_value::ArrayKey::Interned(s) => Value::InternedStr(s),
                    phera_value::ArrayKey::String(s) => Value::String(s),
                }),
                array.len(),
            );
            Some(Value::Array(Rc::new(keys)))
        }
        PureNative::ArrayValues => {
            let [Some(Value::Array(array))] = values else {
                return None;
            };
            if array.is_list() && !array.values().any(Value::is_reference) {
                return Some(Value::Array(array.clone()));
            }
            let vals =
                PhpArray::from_values(array.values().map(|v| v.deref().clone()), array.len());
            Some(Value::Array(Rc::new(vals)))
        }
        PureNative::ArrayMerge => {
            let [Some(Value::Array(a)), Some(Value::Array(b))] = values else {
                return None;
            };
            let mut result = PhpArray::with_capacity(a.len() + b.len());
            for array in [a, b] {
                for (key, value) in array.iter() {
                    if value.is_reference() {
                        return None;
                    }
                    match key {
                        phera_value::ArrayKey::Int(_) => {
                            result.push(value.clone()).ok()?;
                        }
                        phera_value::ArrayKey::Interned(_) | phera_value::ArrayKey::String(_) => {
                            result.insert(key, value.clone());
                        }
                    }
                }
            }
            Some(Value::Array(Rc::new(result)))
        }
        PureNative::Strtoupper => {
            let [s] = values else { return None };
            let bytes = as_str_bytes(s)?;
            if !bytes.iter().any(u8::is_ascii_lowercase) {
                return s.clone();
            }
            Some(Value::from(bytes.to_ascii_uppercase()))
        }
    }
}

fn eval_trim(values: &[Option<Value>], left: bool, right: bool) -> Option<Value> {
    let (original_val, chars) = match values {
        [s] => (s.as_ref()?, DEFAULT_TRIM_BYTES),
        [s, c] => {
            let chars = as_str_bytes(c)?;
            // Ranges like `"a..z"` or invalid `..` warnings fall back.
            if chars.windows(2).any(|w| w == b"..") {
                return None;
            }
            (s.as_ref()?, chars)
        }
        _ => return None,
    };
    let original = match original_val {
        Value::InternedStr(s) => s.as_bytes(),
        Value::String(s) => s.as_bytes(),
        _ => return None,
    };
    let mut trimmed = original;
    if left {
        let start = trimmed
            .iter()
            .position(|b| !chars.contains(b))
            .unwrap_or(trimmed.len());
        trimmed = &trimmed[start..];
    }
    if right {
        let end = trimmed
            .iter()
            .rposition(|b| !chars.contains(b))
            .map_or(0, |last| last + 1);
        trimmed = &trimmed[..end];
    }
    if trimmed.len() == original.len() {
        Some(original_val.clone())
    } else {
        Some(Value::from(trimmed))
    }
}

#[inline]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() == 1 {
        let b = needle[0];
        return haystack.iter().position(|&x| x == b);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let first = needle[0];
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        if let Some(rel) = haystack[i..=limit].iter().position(|&x| x == first) {
            let pos = i + rel;
            if &haystack[pos..pos + needle.len()] == needle {
                return Some(pos);
            }
            i = pos + 1;
        } else {
            break;
        }
    }
    None
}

#[inline]
fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    if needle.len() == 1 {
        let b = needle[0];
        return haystack.iter().rposition(|&x| x == b);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let first = needle[0];
    let limit = haystack.len() - needle.len();
    let mut end = limit + 1;
    while end > 0 {
        if let Some(pos) = haystack[..end].iter().rposition(|&x| x == first) {
            if &haystack[pos..pos + needle.len()] == needle {
                return Some(pos);
            }
            end = pos;
        } else {
            break;
        }
    }
    None
}

#[inline]
fn contained_offset(len: usize, offset: i64) -> Option<usize> {
    let resolved = if offset < 0 {
        i128::from(len as i64) + i128::from(offset)
    } else {
        i128::from(offset)
    };
    if resolved < 0 || resolved > len as i128 {
        None
    } else {
        Some(resolved as usize)
    }
}

#[inline]
fn clamped_range(len: usize, offset: i64, length: Option<i64>) -> std::ops::Range<usize> {
    let len_i = len as i128;
    let start = if offset < 0 {
        (len_i + i128::from(offset)).max(0)
    } else {
        i128::from(offset).min(len_i)
    };
    let end = match length {
        None => len_i,
        Some(n) if n < 0 => (len_i + i128::from(n)).max(start),
        Some(n) => (start + i128::from(n)).min(len_i),
    };
    (start as usize)..(end as usize)
}
