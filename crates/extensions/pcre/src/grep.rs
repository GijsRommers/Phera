//! `preg_grep`: the entries whose value matches, with their keys.
//!
//! ```php
//! preg_grep('/a/', ['x' => 'cat', 'dog']);                    // ['x' => 'cat']
//! preg_grep('/a/', ['x' => 'cat', 'dog'], PREG_GREP_INVERT);  // [0 => 'dog']
//! ```

use crate::errors::Error;
use crate::scan::Scanner;
use crate::{cache, errors};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{Context, NativeResult, PhpArray, Value, args};

pub(crate) fn grep(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(compiled) = cache::get(context, "preg_grep", args::bytes(arguments, 0)) else {
        return Ok(Value::Bool(false));
    };
    errors::set(context, Error::None);
    let invert = args::int(arguments, 2).unwrap_or(0) & 1 != 0;
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 1) {
        for (key, value) in array.iter() {
            let subject = to_php_string(value, context.diagnostics())?;
            match Scanner::new(&compiled, subject.as_bytes(), 0).next() {
                Some(Err(error)) => {
                    errors::set(context, error);
                    break;
                }
                found => {
                    if found.is_some() != invert {
                        result.insert(key, value.clone());
                    }
                }
            }
        }
    }
    Ok(Value::from(result))
}
