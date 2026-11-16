//! Trim operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value, args,
};

use super::bytes;

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
    Both,
}

const TRIM_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, function) in [
        ("trim", trim as SyncFn),
        ("ltrim", left),
        ("rtrim", right),
        ("chop", chop),
    ] {
        registry.function(FunctionEntry::sync(name, TRIM_PARAMS, function));
    }
}

/// Malformed ranges warn independently because PHP still uses the remaining mask bytes.
pub(super) fn mask(context: &mut dyn Context, string: &[u8], name: &str) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if let Some(range) = string.get(cursor..cursor.saturating_add(4))
            && range.get(1) == Some(&b'.')
            && range.get(2) == Some(&b'.')
            && range.get(3).is_some_and(|end| *end >= byte)
        {
            if let Some(&end) = range.get(3) {
                for value in byte..=end {
                    if let Some(entry) = mask.get_mut(usize::from(value)) {
                        *entry = true;
                    }
                }
            }
            cursor += 4;
            continue;
        }
        if byte == b'.' && string.get(cursor + 1) == Some(&b'.') {
            let reason = if cursor == 0 {
                ", no character to the left of '..'"
            } else if cursor + 2 >= string.len() {
                ", no character to the right of '..'"
            } else if string.get(cursor - 1) > string.get(cursor + 2) {
                ", '..'-range needs to be incrementing"
            } else {
                ""
            };
            context.warning(format!("{name}(): Invalid '..'-range{reason}"));
        } else {
            if let Some(entry) = mask.get_mut(usize::from(byte)) {
                *entry = true;
            }
        }
        cursor += 1;
    }
    mask
}

/// Default whitespace excludes form feed in the installed PHP executable.
fn trim_sides(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    side: Side,
) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let characters = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else {
        b" \n\r\t\x0b\0"
    };
    let characters_mask = mask(context, characters, name);
    let mut result = string;
    if matches!(side, Side::Left | Side::Both) {
        while result.first().is_some_and(|b| {
            characters_mask
                .get(usize::from(*b))
                .copied()
                .unwrap_or(false)
        }) {
            result = result.get(1..).unwrap_or_default();
        }
    }
    if matches!(side, Side::Right | Side::Both) {
        while result.last().is_some_and(|b| {
            characters_mask
                .get(usize::from(*b))
                .copied()
                .unwrap_or(false)
        }) {
            result = result.get(..result.len() - 1).unwrap_or_default();
        }
    }
    Ok(bytes(result.to_vec()))
}

fn trim(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    trim_sides(context, arguments, "trim", Side::Both)
}

fn left(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    trim_sides(context, arguments, "ltrim", Side::Left)
}

fn right(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    trim_sides(context, arguments, "rtrim", Side::Right)
}

fn chop(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    trim_sides(context, arguments, "chop", Side::Right)
}
