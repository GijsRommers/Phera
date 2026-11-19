//! Flags and maximum length govern the next line read.
//!
//! ```php
//! $file->setMaxLineLen(80);
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::{change, get};
use crate::{This, files::file::FLAGS_MASK, receiver};

pub(in crate::files::file) fn flags(
    _: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    get(receiver(this)?, |file| Value::Int(file.flags & FLAGS_MASK))
}

pub(in crate::files::file) fn set_flags(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    change(receiver(this)?, |file| {
        file.flags = args::int(arguments, 0).unwrap_or_default()
    })?;
    Ok(Value::Null)
}

pub(in crate::files::file) fn max_line(
    _: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    get(receiver(this)?, |file| {
        Value::Int(file.max_line.unwrap_or(0) as i64)
    })
}

pub(in crate::files::file) fn set_max_line(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    let length = u64::try_from(length).map_err(|_| {
        NativeError::value_error(concat!(
            "SplFileObject::setMaxLineLen(): Argument #1 ($maxLength) ",
            "must be greater than or equal to 0"
        ))
    })?;
    change(receiver(this)?, |file| {
        file.max_line = (length > 0).then_some(length)
    })?;
    Ok(Value::Null)
}
