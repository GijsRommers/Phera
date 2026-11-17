//! Array patterns run sequentially: `preg_replace(['/a/', '/b/'], ['b', 'c'], 'a')`.

use crate::errors::Error;
use crate::scan::Scanner;
use crate::{cache, errors, matches, replacement};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub(crate) fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, false)
}
pub(crate) fn filter(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true)
}
pub(crate) fn values(value: &Value) -> Vec<Value> {
    match &*value.deref() {
        Value::Array(array) => array.values().cloned().collect(),
        value => vec![value.clone()],
    }
}
fn run(context: &mut dyn Context, arguments: &[Value], filter: bool) -> NativeResult {
    let name = if filter {
        "preg_filter"
    } else {
        "preg_replace"
    };
    if args::array(arguments, 0).is_none() && args::array(arguments, 1).is_some() {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #1 ($pattern) must be of type array when argument #2 \
             ($replacement) is an array, string given"
        )));
    }
    let patterns = values(args::value(arguments, 0));
    let replacements = values(args::value(arguments, 1));
    let repeated = args::array(arguments, 1).is_none();
    let limit = args::int(arguments, 3).unwrap_or(-1);
    let mut count = 0;
    let mut apply = |value: &Value| -> NativeResult {
        let mut subject = to_php_string(value, context.diagnostics())?
            .as_bytes()
            .to_vec();
        let old_count = count;
        for (index, pattern) in patterns.iter().enumerate() {
            let pattern = to_php_string(pattern, context.diagnostics())?;
            let replacement = replacements.get(if repeated { 0 } else { index });
            let replacement =
                to_php_string(replacement.unwrap_or(&Value::Null), context.diagnostics())?;
            let Some(compiled) = cache::get(context, name, pattern.as_bytes()) else {
                return Ok(Value::Null);
            };
            errors::set(context, Error::None);
            let mut scanner = Scanner::new(&compiled, &subject, 0);
            let mut output = Vec::new();
            let mut previous_end = 0;
            let mut replaced = 0;
            while limit < 0 || replaced < limit {
                let captures = match scanner.next() {
                    Ok(Some(captures)) => captures,
                    Ok(None) => break,
                    Err(error) => {
                        errors::set(context, error);
                        return Ok(Value::Null);
                    }
                };
                let (start, end) = captures[0].expect("whole match");
                output.extend_from_slice(&subject[previous_end..start]);
                output.extend(replacement::expand(
                    replacement.as_bytes(),
                    &subject,
                    &captures,
                ));
                previous_end = end;
                count += 1;
                replaced += 1;
            }
            output.extend_from_slice(&subject[previous_end..]);
            subject = output;
        }
        Ok(if filter && count == old_count {
            Value::Null
        } else {
            Value::from(subject)
        })
    };
    let result = if let Some(subjects) = args::array(arguments, 2) {
        let mut output = PhpArray::new();
        for (key, value) in subjects.iter() {
            let result = apply(value)?;
            if !result.is_null() {
                output.insert(key, result);
            }
        }
        Value::from(output)
    } else {
        apply(args::value(arguments, 2))?
    };
    matches::finish(arguments, 4, count, result)
}
