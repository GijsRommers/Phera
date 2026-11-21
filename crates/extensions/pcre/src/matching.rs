//! `preg_match` and `preg_match_all`.
//!
//! ```php
//! preg_match('/(\d+)/', 'ab 12 cd 34', $m);       // 1, $m = ['12', '12']
//! preg_match_all('/\d+/', 'ab 12 cd 34', $m);     // 2, $m = [['12', '34']]
//! preg_match('/\d+/', 'ab 12 cd 34', $m, 0, -2);  // 1, $m = ['34']
//! ```

use crate::errors::Error;
use crate::flags::{self, CaptureFlags, Order};
use crate::scan::Scanner;
use crate::{cache, errors, matches};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub(crate) fn single(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, Function::Match)
}

pub(crate) fn all(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, Function::MatchAll)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Function {
    Match,
    MatchAll,
}

impl Function {
    fn name(self) -> &'static str {
        match self {
            Self::Match => "preg_match",
            Self::MatchAll => "preg_match_all",
        }
    }

    /// The shape of `$matches` that the low byte of `$flags` asks for.
    fn order(self, bits: i64) -> Result<Option<Order>, NativeError> {
        match (self, bits & 0xff) {
            (Self::Match, 0) => Ok(None),
            (Self::MatchAll, 0 | flags::PATTERN_ORDER) => Ok(Some(Order::Pattern)),
            (Self::MatchAll, flags::SET_ORDER) => Ok(Some(Order::Set)),
            _ => Err(NativeError::value_error(format!(
                "{}(): Argument #4 ($flags) must be a PREG_* constant",
                self.name()
            ))),
        }
    }

    /// How many matches to look for.
    fn limit(self) -> usize {
        match self {
            Self::Match => 1,
            Self::MatchAll => usize::MAX,
        }
    }
}

fn run(context: &mut dyn Context, arguments: &[Value], function: Function) -> NativeResult {
    let name = function.name();
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
    let captures_requested = args::reference(arguments, 2).is_some();
    if captures_requested {
        matches::assign(arguments, 2, Value::from(PhpArray::new()));
    }
    let flags = args::int(arguments, 3).unwrap_or(0);
    let order = function.order(flags)?;
    let subject = args::bytes(arguments, 1);
    // A negative offset counts from the end, and stops at the start.
    let position = if offset < 0 {
        subject.len().saturating_sub(offset.unsigned_abs() as usize)
    } else {
        offset as usize
    };
    if position > subject.len() {
        errors::set(context, Error::Internal);
        return Ok(Value::Bool(false));
    }
    errors::set(context, Error::None);
    let mut scanner = Scanner::new(&compiled, subject, position);
    if !captures_requested {
        let count = scanner.count_up_to(function.limit()).and_then(|count| {
            if function == Function::Match && arguments.len() < 3 {
                scanner.check_empty_retry()?;
            }
            Ok(count)
        });
        return Ok(match count {
            Ok(count) => Value::Int(count as i64),
            Err(error) => {
                errors::set(context, error);
                Value::Bool(false)
            }
        });
    }
    let capture_flags = CaptureFlags::parse(flags);
    let names = compiled.regex.capture_names();
    let Some(order) = order else {
        let result = match compiled.marks.as_deref() {
            Some(probe) => scanner.next_marked(probe),
            None => scanner
                .next()
                .map(|captures| captures.map(|captures| (captures, None))),
        };
        let (count, row) = match result {
            Some(Ok((captures, mark))) => (
                Value::Int(1),
                matches::row(subject, &captures, names, capture_flags, mark),
            ),
            None => (Value::Int(0), Value::from(PhpArray::new())),
            Some(Err(error)) => {
                errors::set(context, error);
                (Value::Bool(false), Value::from(PhpArray::new()))
            }
        };
        matches::assign(arguments, 2, row);
        return Ok(count);
    };
    let mut rows = Vec::new();
    let mut marks = Vec::new();
    let found = if let Some(probe) = compiled.marks.as_deref() {
        std::iter::from_fn(|| scanner.next_marked(probe))
            .take(function.limit())
            .try_for_each(|found| {
                let (captures, mark) = found?;
                rows.push(captures);
                marks.push(mark);
                Ok(())
            })
    } else {
        scanner
            .by_ref()
            .take(function.limit())
            .try_for_each(|found| {
                rows.push(found?);
                Ok(())
            })
    };
    let result = match found {
        Ok(()) => Value::Int(rows.len() as i64),
        Err(error) => {
            errors::set(context, error);
            Value::Bool(false)
        }
    };
    let table = matches::table(subject, &rows, &marks, names, capture_flags, order);
    matches::assign(arguments, 2, table);
    Ok(result)
}
