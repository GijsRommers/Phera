//! PHP 8.5 weights insertions, replacements and deletions by grapheme.
//!
//! ```php
//! grapheme_levenshtein("é", "e\u{0301}"); // 0
//! grapheme_levenshtein("👩‍💻", "");       // 1
//! ```

use std::cmp::Ordering;

use phera_extension_api::{ConstantValue, Context, FunctionEntry, NativeError, NativeResult};
use phera_extension_api::{Param, ParamType, Registry, Value, args};

use crate::{error, grapheme, grapheme_search};

const FUNCTION: &str = "grapheme_levenshtein";

fn distance(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    grapheme::check_locale(arguments, 5, FUNCTION)?;
    let insertion = args::int(arguments, 2).unwrap_or(1);
    let replacement = args::int(arguments, 3).unwrap_or(1);
    let deletion = args::int(arguments, 4).unwrap_or(1);
    for (number, name, value) in [
        (3, "insertion_cost", insertion),
        (4, "replacement_cost", replacement),
        (5, "deletion_cost", deletion),
    ] {
        if !(1..=grapheme::LIMIT).contains(&value) {
            return Err(NativeError::value_error(format!(
                "{FUNCTION}(): Argument #{number} (${name}) must be greater than 0 \
                 and less than or equal to {}",
                grapheme::LIMIT,
            )));
        }
    }
    let mut first = args::bytes(arguments, 0);
    let mut second = args::bytes(arguments, 1);
    if std::str::from_utf8(first).is_err() || std::str::from_utf8(second).is_err() {
        return grapheme::input_error(context, FUNCTION, Value::Bool(false));
    }
    if first.len() < second.len() && insertion == replacement && replacement == deletion {
        std::mem::swap(&mut first, &mut second);
    }
    let Some(collator) = grapheme_search::collator(args::bytes(arguments, 5), false) else {
        return error::fail(
            context,
            FUNCTION,
            error::ILLEGAL_ARGUMENT,
            "Error on ucol_open",
            Value::Bool(false),
        );
    };
    let second_boundaries: Vec<_> = grapheme::boundaries(second).collect();
    let mut previous: Vec<_> = (0..second_boundaries.len())
        .map(|index| index as i64 * insertion)
        .collect();
    let mut current = vec![0; previous.len()];
    let mut start = 0;
    for end in grapheme::boundaries(first).skip(1) {
        let left = &first[start..end];
        current[0] = previous[0] + deletion;
        for (column, part) in second_boundaries.windows(2).enumerate() {
            let right = &second[part[0]..part[1]];
            let cost = if collator.compare_utf8(left, right) == Ordering::Equal {
                0
            } else {
                replacement
            };
            current[column + 1] = (previous[column] + cost)
                .min(previous[column + 1] + deletion)
                .min(current[column] + insertion);
        }
        std::mem::swap(&mut current, &mut previous);
        start = end;
    }
    Ok(Value::Int(previous.last().copied().unwrap_or(0)))
}

pub fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
        Param::new("insertion_cost", ParamType::Int).with_default(ConstantValue::Int(1)),
        Param::new("replacement_cost", ParamType::Int).with_default(ConstantValue::Int(1)),
        Param::new("deletion_cost", ParamType::Int).with_default(ConstantValue::Int(1)),
        grapheme::LOCALE,
    ];
    registry.function(FunctionEntry::sync(FUNCTION, PARAMETERS, distance));
    registry.describe_function(FUNCTION, "int|false", None);
}
