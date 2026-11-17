//! Search returns character positions: `mb_strpos('éabc', 'b') === 2`.
use crate::{codec::decode, text};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};
#[derive(Clone, Copy)]
pub enum Search {
    First,
    Last,
    FirstFold,
    LastFold,
}
impl Search {
    fn reverse(self) -> bool {
        matches!(self, Self::Last | Self::LastFold)
    }
    fn fold(self) -> bool {
        matches!(self, Self::FirstFold | Self::LastFold)
    }
}
pub fn position(ctx: &mut dyn Context, a: &[Value], name: &str, mode: Search) -> NativeResult {
    let encoding = crate::encoding::argument(ctx, a, 3, name)?;
    let haystack = crate::search_bytes::searchable(args::bytes(a, 0), encoding, mode.fold());
    let needle = crate::search_bytes::searchable(args::bytes(a, 1), encoding, mode.fold());
    let found = crate::search_bytes::find(
        &haystack,
        &needle,
        args::int(a, 2).unwrap_or(0),
        mode.reverse(),
    )
    .map_err(|()| {
        NativeError::value_error(format!(
            "{name}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        ))
    })?;
    Ok(found.map_or(Value::Bool(false), |n| Value::Int(n as i64)))
}

pub fn portion(ctx: &mut dyn Context, a: &[Value], name: &str, mode: Search) -> NativeResult {
    let encoding = crate::encoding::argument(ctx, a, 3, name)?;
    let haystack = crate::search_bytes::searchable(args::bytes(a, 0), encoding, mode.fold());
    let needle = crate::search_bytes::searchable(args::bytes(a, 1), encoding, mode.fold());
    let Some(index) = crate::search_bytes::find(&haystack, &needle, 0, mode.reverse())
        .ok()
        .flatten()
    else {
        return Ok(Value::Bool(false));
    };
    let before = args::bool(a, 2).unwrap_or(false);
    text::substr(
        ctx,
        &[
            args::value(a, 0).clone(),
            Value::Int(if before { 0 } else { index as i64 }),
            if before {
                Value::Int(index as i64)
            } else {
                Value::Null
            },
            Value::from(encoding.name()),
        ],
    )
}

pub fn count(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (haystack, encoding) = text::points(ctx, a, 2, "mb_substr_count")?;
    let needle = decode(args::bytes(a, 1), encoding);
    if needle.is_empty() {
        return Err(NativeError::value_error(
            "mb_substr_count(): Argument #2 ($needle) must not be empty",
        ));
    }
    let mut count = 0;
    let mut offset = 0;
    while offset < haystack.len() {
        if haystack[offset..].starts_with(&needle) {
            count += 1;
            offset += needle.len();
        } else {
            offset += 1;
        }
    }
    Ok(Value::Int(count))
}
