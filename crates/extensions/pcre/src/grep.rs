//! `preg_grep`: the entries whose value matches, with their keys.
//!
//! ```php
//! preg_grep('/a/', ['x' => 'cat', 'dog']);                    // ['x' => 'cat']
//! preg_grep('/a/', ['x' => 'cat', 'dog'], PREG_GREP_INVERT);  // [0 => 'dog']
//! ```

use crate::errors::Error;
use crate::scan::Scanner;
use crate::{cache, errors, flags};
use phera_extension_api::{Context, LocalFuture, NativeResult, PhpArray, Value, args};

pub(crate) fn grep<const RETURN_FALSE: bool>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        matching_entries::<RETURN_FALSE>(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("preg_grep", &arguments, error))
    })
}

async fn matching_entries<const RETURN_FALSE: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    let Some(compiled) = cache::get(context, "preg_grep", args::bytes(arguments, 0)) else {
        return Ok(Value::Bool(false));
    };
    errors::set(context, Error::None);
    let invert = args::int(arguments, 2).unwrap_or(0) & flags::GREP_INVERT != 0;
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 1) {
        for (key, value) in array.iter() {
            let subject = context.stringify(value).await?;
            match Scanner::new(&compiled, subject.as_bytes(), 0).next() {
                Some(Err(error)) => {
                    errors::set(context, error);
                    if RETURN_FALSE {
                        return Ok(Value::Bool(false));
                    }
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
