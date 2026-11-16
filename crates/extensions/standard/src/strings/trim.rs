//! Trim operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value, args,
};

use super::bytes;

const PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String).optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    for (n, f) in [
        ("trim", trim as SyncFn),
        ("ltrim", left),
        ("rtrim", right),
        ("chop", chop),
    ] {
        registry.function(FunctionEntry::sync(n, PARAMS, f));
    }
}

/// Malformed ranges warn independently because PHP still uses the remaining mask bytes.
pub(super) fn mask(context: &mut dyn Context, string: &[u8], name: &str) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut i = 0;
    while let Some(&b) = string.get(i) {
        if let Some(t) = string.get(i..i.saturating_add(4))
            && t.get(1) == Some(&b'.')
            && t.get(2) == Some(&b'.')
            && t.get(3).is_some_and(|e| *e >= b)
        {
            if let Some(&end) = t.get(3) {
                for value in b..=end {
                    mask[usize::from(value)] = true;
                }
            }
            i += 4;
            continue;
        }
        if b == b'.' && string.get(i + 1) == Some(&b'.') {
            let reason = if i == 0 {
                ", no character to the left of '..'"
            } else if i + 2 >= string.len() {
                ", no character to the right of '..'"
            } else if string.get(i - 1) > string.get(i + 2) {
                ", '..'-range needs to be incrementing"
            } else {
                ""
            };
            context.warning(format!("{name}(): Invalid '..'-range{reason}"));
        } else {
            mask[usize::from(b)] = true;
        }
        i += 1;
    }
    mask
}

/// Default whitespace excludes form feed in the installed PHP executable.
fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    trim_left: bool,
    trim_right: bool,
) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let characters = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else {
        b" \n\r\t\x0b\0"
    };
    let characters_mask = mask(context, characters, name);
    let mut result = string;
    if trim_left {
        while result
            .first()
            .is_some_and(|b| characters_mask[usize::from(*b)])
        {
            result = result.get(1..).unwrap_or_default();
        }
    }
    if trim_right {
        while result
            .last()
            .is_some_and(|b| characters_mask[usize::from(*b)])
        {
            result = result.get(..result.len() - 1).unwrap_or_default();
        }
    }
    Ok(bytes(result.to_vec()))
}

fn trim(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "trim", true, true)
}

fn left(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "ltrim", true, false)
}

fn right(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "rtrim", false, true)
}

fn chop(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "chop", false, true)
}
