//! Named matches have both keys: `preg_match('/(?<word>a)/', 'a', $m)`.

use crate::flags::{CaptureFlags, Order};
use crate::scan::Captures;
use phera_extension_api::{ArrayKey, NativeResult, PhpArray, Value, args};

pub(crate) fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .collect::<PhpArray>(),
    )
}
pub(crate) fn assign(arguments: &[Value], index: usize, value: Value) {
    if let Some(reference) = args::reference(arguments, index) {
        *reference.borrow_mut() = value;
    }
}
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
pub(crate) fn row(
    subject: &[u8],
    captures: &Captures,
    names: &[Option<String>],
    flags: CaptureFlags,
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
            if span.is_some() || !result.contains_key(&key) {
                result.insert(key, value.clone());
            }
        }
        result.insert(ArrayKey::Int(index as i64), value);
    }
    Value::from(result)
}
pub(crate) fn table(
    subject: &[u8],
    rows: &[Captures],
    names: &[Option<String>],
    flags: CaptureFlags,
    order: Order,
) -> Value {
    match order {
        Order::Single => rows.first().map_or_else(
            || Value::from(PhpArray::new()),
            |captures| row(subject, captures, names, flags),
        ),
        Order::Set => list(
            rows.iter()
                .map(|captures| row(subject, captures, names, flags)),
        ),
        Order::Pattern => {
            let mut result = PhpArray::new();
            for (index, name) in names.iter().enumerate() {
                let column = list(rows.iter().map(|row| captured(subject, row[index], flags)));
                if let Some(name) = name {
                    result.insert(ArrayKey::str(name.as_str()), column.clone());
                }
                result.insert(ArrayKey::Int(index as i64), column);
            }
            Value::from(result)
        }
    }
}
pub(crate) fn finish(
    arguments: &[Value],
    index: usize,
    count: usize,
    result: Value,
) -> NativeResult {
    assign(arguments, index, Value::Int(count as i64));
    Ok(result)
}
