//! Names and IDs as exposed by PHP, in their original order.
//!
//! ```php
//! filter_id("boolean"); // FILTER_VALIDATE_BOOL
//! filter_id("Boolean"); // false
//! ```

use phera_extension_api::{Context, NativeResult, PhpArray, Value, args};

pub const FILTERS: &[(&str, i64)] = &[
    ("int", 257),
    ("boolean", 258),
    ("float", 259),
    ("validate_regexp", 272),
    ("validate_domain", 277),
    ("validate_url", 273),
    ("validate_email", 274),
    ("validate_ip", 275),
    ("validate_mac", 276),
    ("string", 513),
    ("stripped", 513),
    ("encoded", 514),
    ("special_chars", 515),
    ("full_special_chars", 522),
    ("unsafe_raw", 516),
    ("email", 517),
    ("url", 518),
    ("number_int", 519),
    ("number_float", 520),
    ("add_slashes", 523),
    ("callback", 1024),
];

pub fn list(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for &(name, _) in FILTERS {
        result.push(Value::from(name));
    }
    Ok(Value::from(result))
}

pub fn id(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    // PHP compares the name as a C string.
    let name = args::bytes(arguments, 0).split(|&byte| byte == 0).next();
    Ok(FILTERS
        .iter()
        .find(|(candidate, _)| Some(candidate.as_bytes()) == name)
        .map_or(Value::Bool(false), |&(_, id)| Value::Int(id)))
}
