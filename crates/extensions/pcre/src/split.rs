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
    // The number of pieces, the last being the rest of the subject; -1 and
    // 0 mean no limit.
    let mut pieces_left = match args::int(arguments, 2).unwrap_or(-1) {
        -1 | 0 => None,
        limit => Some(limit),
    };
    let mut scanner = Scanner::new(&compiled, subject, 0);
    let mut pieces = Vec::new();
    let mut previous_end = 0;
    errors::set(context, Error::None);
    while pieces_left.is_none_or(|left| left > 1) {
        let captures = match scanner.next() {
            Some(Ok(captures)) => captures,
            None => break,
            Some(Err(error)) => {
                errors::set(context, error);
                return Ok(Value::Bool(false));
            }
        };
        let (start, end) = captures[0].expect("group 0 is the whole match");
        if !flags.no_empty || start != previous_end {
            pieces.push(matches::captured(
                subject,
                Some((previous_end, start)),
                capture_flags,
            ));
            if let Some(left) = &mut pieces_left {
                *left -= 1;
            }
        }
        if flags.delimiters {
            let groups = captures.iter().rposition(Option::is_some).unwrap_or(0) + 1;
            for span in captures.iter().take(groups).skip(1) {
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
