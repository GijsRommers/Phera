//! Length, case and byte order: `strtoupper("abc")` is `"ABC"`.
//!
//! PHP strings are bytes, and since PHP 8.2 these functions change the case
//! of the ASCII letters only, whatever the locale. A UTF-8 `é` keeps its
//! case, and `strrev` reverses the two bytes it is made of.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    STRLEN, STRTOUPPER, STRTOLOWER, UCFIRST, LCFIRST, UCWORDS, STRREV,
];

/// `strlen(string $string): int`, the length in bytes.
///
/// ```php
/// strlen("héllo"); // 6: é takes two bytes in UTF-8
/// ```
const STRLEN: FunctionEntry =
    FunctionEntry::sync("strlen", &[Param::new("string", ParamType::String)], strlen);

fn strlen(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(args::bytes(arguments, 0).len() as i64))
}

/// `strtoupper(string $string): string`
///
/// ```php
/// strtoupper("abc é"); // "ABC é"
/// ```
const STRTOUPPER: FunctionEntry = FunctionEntry::sync(
    "strtoupper",
    &[Param::new("string", ParamType::String)],
    strtoupper,
);

fn strtoupper(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(args::bytes(arguments, 0).to_ascii_uppercase()))
}

/// `strtolower(string $string): string`
///
/// ```php
/// strtolower("ABC É"); // "abc É"
/// ```
const STRTOLOWER: FunctionEntry = FunctionEntry::sync(
    "strtolower",
    &[Param::new("string", ParamType::String)],
    strtolower,
);

fn strtolower(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(args::bytes(arguments, 0).to_ascii_lowercase()))
}

/// `ucfirst(string $string): string`, the first byte in upper case.
///
/// ```php
/// ucfirst("hello world"); // "Hello world"
/// ```
const UCFIRST: FunctionEntry = FunctionEntry::sync(
    "ucfirst",
    &[Param::new("string", ParamType::String)],
    ucfirst,
);

fn ucfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(first) = string.first_mut() {
        first.make_ascii_uppercase();
    }
    Ok(Value::from(string))
}

/// `lcfirst(string $string): string`, the first byte in lower case.
///
/// ```php
/// lcfirst("Hello World"); // "hello World"
/// ```
const LCFIRST: FunctionEntry = FunctionEntry::sync(
    "lcfirst",
    &[Param::new("string", ParamType::String)],
    lcfirst,
);

fn lcfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(first) = string.first_mut() {
        first.make_ascii_lowercase();
    }
    Ok(Value::from(string))
}

/// `ucwords(string $string, string $separators = " \t\r\n\f\v"): string`,
/// the first byte and every byte after a separator in upper case.
///
/// ```php
/// ucwords("hello big-world");      // "Hello Big-world"
/// ucwords("hello big-world", "-"); // "Hello big-World"
/// ```
const UCWORDS: FunctionEntry = FunctionEntry::sync(
    "ucwords",
    &[
        Param::new("string", ParamType::String),
        Param::new("separators", ParamType::String).optional(),
    ],
    ucwords,
);

fn ucwords(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let separators = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else {
        b" \t\r\n\x0c\x0b"
    };
    let mut after_separator = true;
    let result: Vec<u8> = args::bytes(arguments, 0)
        .iter()
        .map(|&byte| {
            let changed = if after_separator {
                byte.to_ascii_uppercase()
            } else {
                byte
            };
            after_separator = separators.contains(&byte);
            changed
        })
        .collect();
    Ok(Value::from(result))
}

/// `strrev(string $string): string`, the bytes in reverse order.
///
/// ```php
/// strrev("abc"); // "cba"
/// ```
const STRREV: FunctionEntry =
    FunctionEntry::sync("strrev", &[Param::new("string", ParamType::String)], strrev);

fn strrev(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    string.reverse();
    Ok(Value::from(string))
}
