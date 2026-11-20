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
    fn order(self, bits: i64) -> Result<Order, NativeError> {
        match (self, bits & 0xff) {
            (Self::Match, 0) => Ok(Order::Single),
            (Self::MatchAll, 0 | flags::PATTERN_ORDER) => Ok(Order::Pattern),
            (Self::MatchAll, flags::SET_ORDER) => Ok(Order::Set),
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
    matches::assign(arguments, 2, Value::from(PhpArray::new()));
    let flags = args::int(arguments, 3).unwrap_or(0);
    let order = function.order(flags)?;
    let capture_flags = CaptureFlags::parse(flags);
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
    let names = compiled.regex.capture_names();
    let mut scanner = Scanner::new(&compiled, subject, position);
    let mut rows = Vec::new();
    let mut marks = Vec::new();
    let found = if let Some(probe) = compiled.marks.as_deref().filter(|_| arguments.len() >= 3) {
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
    if let Err(error) = found {
        errors::set(context, error);
        let table = matches::table(subject, &rows, &marks, names, capture_flags, order);
        matches::assign(arguments, 2, table);
        return Ok(Value::Bool(false));
    }
    if function == Function::Match
        && arguments.len() < 3
        && let Err(error) = scanner.check_empty_retry()
    {
        errors::set(context, error);
        return Ok(Value::Bool(false));
    }
    let table = matches::table(subject, &rows, &marks, names, capture_flags, order);
    matches::assign(arguments, 2, table);
    Ok(Value::Int(rows.len() as i64))
}
