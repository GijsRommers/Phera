//! `preg_match_all('/a/', 'aba', $m)` returns 2 and fills pattern-ordered matches.

use crate::errors::Error;
use crate::scan::Scanner;
use crate::{cache, errors, flags, matches};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub(crate) fn single(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, false)
}
pub(crate) fn all(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true)
}
fn run(context: &mut dyn Context, arguments: &[Value], global: bool) -> NativeResult {
    let name = if global {
        "preg_match_all"
    } else {
        "preg_match"
    };
    let Some(compiled) = cache::get(context, name, args::bytes(arguments, 0)) else {
        return Ok(Value::Bool(false));
    };
    let offset = args::int(arguments, 4).unwrap_or(0);
    if offset == i64::MIN {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #5 ($offset) must be greater than {}",
            i64::MIN
        )));
    }
    matches::assign(arguments, 2, Value::from(PhpArray::new()));
    let (order, flags) = flags::match_flags(name, args::int(arguments, 3).unwrap_or(0), global)?;
    let subject = args::bytes(arguments, 1);
    let offset = args::int(arguments, 4).unwrap_or(0);
    let position = if offset < 0 {
        subject.len().saturating_sub(offset.unsigned_abs() as usize)
    } else {
        offset as usize
    };
    if position > subject.len() {
        errors::set(context, Error::Internal);
        return Ok(Value::Bool(false));
    }
    let mut scanner = Scanner::new(&compiled, subject, position);
    let mut rows = Vec::new();
    errors::set(context, Error::None);
    loop {
        match scanner.next() {
            Ok(Some(captures)) => rows.push(captures),
            Ok(None) => break,
            Err(error) => {
                errors::set(context, error);
                matches::assign(
                    arguments,
                    2,
                    matches::table(subject, &rows, compiled.regex.capture_names(), flags, order),
                );
                return Ok(Value::Bool(false));
            }
        }
        if !global {
            if arguments.len() < 3
                && let Err(error) = scanner.check_empty_retry()
            {
                errors::set(context, error);
                return Ok(Value::Bool(false));
            }
            break;
        }
    }
    matches::assign(
        arguments,
        2,
        matches::table(subject, &rows, compiled.regex.capture_names(), flags, order),
    );
    Ok(Value::Int(rows.len() as i64))
}
