//! `preg_split('/(,)/', 'a,b', -1, PREG_SPLIT_DELIM_CAPTURE)` includes the comma.

use crate::errors::Error;
use crate::flags::{CaptureFlags, SplitFlags};
use crate::scan::Scanner;
use crate::{cache, errors, matches};
use phera_extension_api::{Context, NativeResult, Value, args};

pub(crate) fn split(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(compiled) = cache::get(context, "preg_split", args::bytes(arguments, 0)) else {
        return Ok(Value::Bool(false));
    };
    let subject = args::bytes(arguments, 1);
    let flags = SplitFlags::parse(args::int(arguments, 3).unwrap_or(0));
    let capture_flags = CaptureFlags {
        offsets: flags.offsets,
        unmatched_null: false,
    };
    let mut remaining = args::int(arguments, 2).unwrap_or(-1);
    if remaining == 0 {
        remaining = -1;
    }
    let mut scanner = Scanner::new(&compiled, subject, 0);
    let mut pieces = Vec::new();
    let mut previous_end = 0;
    errors::set(context, Error::None);
    while remaining == -1 || remaining > 1 {
        let captures = match scanner.next() {
            Ok(Some(captures)) => captures,
            Ok(None) => break,
            Err(error) => {
                errors::set(context, error);
                return Ok(Value::Bool(false));
            }
        };
        let (start, end) = captures[0].expect("whole match");
        if !flags.no_empty || start != previous_end {
            pieces.push(matches::captured(
                subject,
                Some((previous_end, start)),
                capture_flags,
            ));
            if remaining != -1 {
                remaining -= 1;
            }
        }
        if flags.delimiters {
            let count = captures.iter().rposition(Option::is_some).unwrap_or(0) + 1;
            for span in captures.iter().take(count).skip(1) {
                if !flags.no_empty || span.is_some_and(|(start, end)| start != end) {
                    pieces.push(matches::captured(subject, *span, capture_flags));
                }
            }
        }
        previous_end = end;
    }
    if !flags.no_empty || previous_end != subject.len() {
        pieces.push(matches::captured(
            subject,
            Some((previous_end, subject.len())),
            capture_flags,
        ));
    }
    Ok(matches::list(pieces))
}
