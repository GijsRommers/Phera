//! `dirname('a/b/c', 2)` is `'a'`; `pathinfo('a/b.txt')` separates the suffix.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    PhpString, Registry, Value, args,
};

use crate::mutations::PATH;

const DIRNAME: i64 = 1;
const BASENAME: i64 = 2;
const EXTENSION: i64 = 4;
const FILENAME: i64 = 8;
const ALL: i64 = DIRNAME | BASENAME | EXTENSION | FILENAME;

pub fn register(registry: &mut Registry) {
    const DIR_PARAMS: &[Param] = &[PATH, Param::new("levels", ParamType::Int).optional()];
    const INFO_PARAMS: &[Param] = &[PATH, Param::new("flags", ParamType::Int).optional()];
    registry.function(FunctionEntry::sync("dirname", DIR_PARAMS, dirname));
    let info = if registry
        .level()
        .has(phera_extension_api::Change::PathInfoFlags)
    {
        pathinfo::<true>
    } else {
        pathinfo::<false>
    };
    registry.function(FunctionEntry::sync("pathinfo", INFO_PARAMS, info));
    for (name, value) in [
        ("PATHINFO_DIRNAME", DIRNAME),
        ("PATHINFO_BASENAME", BASENAME),
        ("PATHINFO_EXTENSION", EXTENSION),
        ("PATHINFO_FILENAME", FILENAME),
        ("PATHINFO_ALL", ALL),
    ] {
        registry.constant(name, value);
    }
}

fn dirname(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let levels = args::int(arguments, 1).unwrap_or(1);
    if levels < 1 {
        return Err(NativeError::value_error(
            "dirname(): Argument #2 ($levels) must be greater than or equal to 1",
        ));
    }
    let mut path = args::bytes(arguments, 0);
    for _ in 0..levels {
        let parent = parent(path);
        if parent == path {
            break;
        }
        path = parent;
    }
    Ok(Value::from(PhpString::from(path)))
}

fn separator(byte: u8) -> bool {
    byte == b'/' || (cfg!(windows) && byte == b'\\')
}

/// Directory names are lexical, including written "." and ".." components.
pub fn parent(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return path;
    }
    let Some(end) = path.iter().rposition(|byte| !separator(*byte)) else {
        return &path[..1];
    };
    let Some(boundary) = path[..end].iter().rposition(|byte| separator(*byte)) else {
        return b".";
    };
    let prefix = &path[..boundary];
    prefix
        .iter()
        .rposition(|byte| !separator(*byte))
        .map_or(&path[..1], |last| &prefix[..=last])
}

pub fn basename(path: &[u8]) -> &[u8] {
    let Some(end) = path.iter().rposition(|byte| !separator(*byte)) else {
        return b"";
    };
    let start = path[..end]
        .iter()
        .rposition(|byte| separator(*byte))
        .map_or(0, |at| at + 1);
    &path[start..=end]
}

fn pathinfo<const VALIDATE_FLAGS: bool>(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let path = args::bytes(args, 0);
    let flags = args::int(args, 1).unwrap_or(ALL);
    if VALIDATE_FLAGS {
        if !(DIRNAME..=ALL).contains(&flags) {
            return Err(NativeError::value_error(
                "pathinfo(): Argument #2 ($flags) must be one of the PATHINFO_* constants",
            ));
        }
        if flags != ALL && flags.count_ones() != 1 {
            return Err(NativeError::value_error(
                "pathinfo(): Argument #2 ($flags) must be only one of the PATHINFO_* constants",
            ));
        }
    }
    let directory = parent(path)
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
    let base = basename(path);
    let dot = base.iter().rposition(|&byte| byte == b'.');
    let extension = dot.map(|dot| &base[dot + 1..]);
    let name = dot.map_or(base, |dot| &base[..dot]);
    let parts = [
        (
            DIRNAME,
            "dirname",
            (!directory.is_empty()).then_some(directory),
        ),
        (BASENAME, "basename", Some(base)),
        (EXTENSION, "extension", extension),
        (FILENAME, "filename", Some(name)),
    ];
    if flags != ALL {
        let value = parts
            .into_iter()
            .find_map(|(flag, _, value)| if flags & flag != 0 { value } else { None });
        return Ok(Value::from(PhpString::from(value.unwrap_or_default())));
    }
    let mut result = PhpArray::new();
    for (_, key, value) in parts {
        if let Some(value) = value {
            result.insert(ArrayKey::str(key), Value::from(PhpString::from(value)));
        }
    }
    Ok(Value::from(result))
}
