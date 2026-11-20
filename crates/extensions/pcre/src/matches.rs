//! Building `$matches` from the spans of each match.
//!
//! ```php
//! preg_match('/(?<word>a)(b)?/', 'a', $m);
//! // $m = [0 => 'a', 'word' => 'a', 1 => 'a']: a named group is there by
//! // name and by number, and the trailing group 2, unmatched, is left out
//! ```

use crate::flags::{CaptureFlags, Order};
use crate::scan::Captures;
use phera_extension_api::{ArrayKey, NativeResult, PhpArray, Value, args};

/// A PHP list of `values`.
pub(crate) fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .collect::<PhpArray>(),
    )
}

/// Writes `value` into the by-reference argument at `index`, if the
/// caller passed one.
pub(crate) fn assign(arguments: &[Value], index: usize, value: Value) {
    if let Some(reference) = args::reference(arguments, index) {
        *reference.borrow_mut() = value;
    }
}

/// One group as PHP shows it: its text (`""`, or null with
/// `PREG_UNMATCHED_AS_NULL`, when it did not take part), or with
/// `PREG_OFFSET_CAPTURE` the pair `[text, offset]`, the offset -1 then.
pub(crate) fn captured(subject: &[u8], span: Option<(usize, usize)>, flags: CaptureFlags) -> Value {
    let value = match span {
        Some((start, end)) => Value::from(subject[start..end].to_vec()),
        None if flags.unmatched_null => Value::Null,
        None => Value::from(""),
    };
    if flags.offsets {
        list([
            value,
            Value::Int(span.map_or(-1, |(start, _)| start as i64)),
        ])
    } else {
        value
    }
}

/// One match: each group by number, and a named group by its name too.
/// Trailing groups that did not take part are left out, unless
/// `PREG_UNMATCHED_AS_NULL`.
pub(crate) fn row(
    subject: &[u8],
    captures: &Captures,
    names: &[Option<String>],
    flags: CaptureFlags,
    mark: Option<&[u8]>,
) -> Value {
    let length = if flags.unmatched_null {
        captures.len()
    } else {
        captures
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1)
    };
    let mut result = PhpArray::new();
    for (index, span) in captures.iter().take(length).enumerate() {
        let value = captured(subject, *span, flags);
        if let Some(name) = &names[index] {
            let key = ArrayKey::str(name.as_str());
            // With /J two groups may share a name; one that did not take
            // part does not overwrite one that did.
            if span.is_some() || !result.contains_key(&key) {
                result.insert(key, value.clone());
            }
        }
        result.insert(ArrayKey::Int(index as i64), value);
    }
    if let Some(name) = mark {
        result.insert(ArrayKey::str("MARK"), Value::from(name.to_vec()));
    }
    Value::from(result)
}

/// `$matches` in the shape `order` asks for.
pub(crate) fn table(
    subject: &[u8],
    rows: &[Captures],
    marks: &[Option<&[u8]>],
    names: &[Option<String>],
    flags: CaptureFlags,
    order: Order,
) -> Value {
    match order {
        Order::Single => rows.first().map_or_else(
            || Value::from(PhpArray::new()),
            |captures| {
                row(
                    subject,
                    captures,
                    names,
                    flags,
                    marks.first().copied().flatten(),
                )
            },
        ),
        Order::Set => list(rows.iter().enumerate().map(|(index, captures)| {
            row(
                subject,
                captures,
                names,
                flags,
                marks.get(index).copied().flatten(),
            )
        })),
        Order::Pattern => {
            let mut result = PhpArray::new();
            for (index, name) in names.iter().enumerate() {
                let column = list(rows.iter().map(|row| captured(subject, row[index], flags)));
                if let Some(name) = name {
                    result.insert(ArrayKey::str(name.as_str()), column.clone());
                }
                result.insert(ArrayKey::Int(index as i64), column);
            }
            if !marks.is_empty() {
                let column: PhpArray = marks
                    .iter()
                    .enumerate()
                    .filter_map(|(index, mark)| {
                        Some((ArrayKey::Int(index as i64), Value::from((*mark)?.to_vec())))
                    })
                    .collect();
                if !column.is_empty() {
                    result.insert(ArrayKey::str("MARK"), Value::from(column));
                }
            }
            Value::from(result)
        }
    }
}

/// Writes `$count` into the argument at `index` and returns `result`.
pub(crate) fn finish(
    arguments: &[Value],
    index: usize,
    count: usize,
    result: Value,
) -> NativeResult {
    assign(arguments, index, Value::Int(count as i64));
    Ok(result)
}
