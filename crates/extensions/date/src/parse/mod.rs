//! Hand-written timelib scanner: `strtotime('next Monday', $base)` and `date_parse()`.
//! Every family offers its longest match; ties use parse_date.re's rule order.
mod actions;
pub(crate) mod apply;
mod compound;
mod dates;
mod scan;
mod short_time;
pub(crate) mod state;
mod time;
pub(crate) mod token;
mod words;

use phera_extension_api::{Context, NativeResult, Value, args};
use scan::{Cursor, offer};
use state::State;
use token::Action;

pub fn date_parse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(parse(args::bytes(arguments, 0)).array())
}
pub fn strtotime(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    if input.is_empty() {
        return Ok(Value::Bool(false));
    }
    let parsed = parse(input);
    if !parsed.parsed.errors.is_empty() {
        return Ok(Value::Bool(false));
    }
    let base = args::int(arguments, 1).unwrap_or_else(crate::functions::now);
    Ok(
        apply::timestamp(&parsed, base, &crate::zone::current(context))
            .map_or(Value::Bool(false), Value::Int),
    )
}

pub(crate) fn parse(input: &[u8]) -> State {
    let whitespace = |b: &u8| {
        matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)
            || (cfg!(target_os = "macos") && *b == 0xa0)
    };
    let start = input
        .iter()
        .position(|b| !whitespace(b))
        .unwrap_or(input.len());
    let end = input
        .iter()
        .rposition(|b| !whitespace(b))
        .map_or(start, |p| p + 1);
    let input = &input[start..end];
    let mut state = State::default();
    if input.is_empty() {
        state.error(0, "Empty string");
        return state;
    }
    let mut position = 0;
    while position < input.len() {
        let rest = &input[position..];
        let mut best = None;
        // Dispatch avoids evaluating impossible families on each byte.
        if rest[0].is_ascii_alphanumeric() || b"+-@ \t".contains(&rest[0]) {
            words::candidates(rest, &mut best);
            if rest[0].is_ascii_alphanumeric() || b"+-".contains(&rest[0]) {
                dates::candidates(rest, &mut best);
                time::candidates(rest, &mut best);
                compound::candidates(rest, &mut best);
            }
        }
        let len = time::zone_len(rest);
        if len > 0 {
            offer(
                &mut best,
                41,
                Cursor {
                    input: rest,
                    pos: len,
                },
                Action::Zone,
            );
        }
        if let Some(candidate) = best {
            actions::apply(
                &mut state,
                candidate.action,
                &rest[..candidate.len.min(rest.len())],
                position,
            );
            position += candidate.len;
        } else {
            let mut c = Cursor::new(rest);
            if c.space().is_some() {
                position += c.pos;
            } else {
                if !b".,\n\0".contains(&rest[0]) {
                    state.error(position, "Unexpected character");
                }
                position += 1;
            }
        }
    }
    state.finish(input.len() + 1);
    for field in &mut state.parsed.fields {
        if *field == Some(-9999999) {
            *field = None;
        }
    }
    state
}
