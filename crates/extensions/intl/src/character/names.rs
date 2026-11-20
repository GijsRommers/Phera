//! Resolve the Unicode names used by IntlChar in both directions.
//!
//! ```php
//! IntlChar::charName('☃');          // "SNOWMAN"
//! IntlChar::charFromName('snowman'); // 9731
//! ```

mod table;

use icu_properties::{CodePointMapData, CodePointSetData, props};
use phera_extension_api::{Context, LocalFuture, NativeResult, Value, args};

use super::codepoint;
use crate::error;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Unicode,
    Old,
    Extended,
    Alias,
}

impl Choice {
    fn read(values: &[Value], index: usize) -> Option<Self> {
        match args::int(values, index).unwrap_or(0) {
            0 => Some(Self::Unicode),
            1 => Some(Self::Old),
            2 => Some(Self::Extended),
            3 => Some(Self::Alias),
            _ => None,
        }
    }
}

pub(super) fn name(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    error::clear(context);
    const SITE: &str = "IntlChar::charName";
    let code = match codepoint(args::value(values, 0)) {
        Ok(code) => code,
        Err(detail) => return error::fail(context, SITE, 1, detail, Value::Null),
    };
    let Some(choice) = Choice::read(values, 1) else {
        return error::fail(
            context,
            SITE,
            1,
            "Failure getting character name",
            Value::Null,
        );
    };
    Ok(Value::from(lookup(code, choice).unwrap_or_default()))
}

pub(super) fn from_name(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    error::clear(context);
    let name = args::bytes(values, 0)
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
    let Some(choice) = Choice::read(values, 1).filter(|_| !name.is_empty()) else {
        error::warning_code(context, 1);
        return Ok(Value::Null);
    };
    let code = std::str::from_utf8(name)
        .ok()
        .and_then(|name| reverse_lookup(name, choice));
    if let Some(code) = code {
        Ok(Value::Int(i64::from(code)))
    } else {
        error::warning_code(context, 12);
        Ok(Value::Null)
    }
}

pub(super) fn enumerate(
    context: &mut dyn Context,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const SITE: &str = "IntlChar::enumCharNames";
        error::clear(context);
        let read = |index| codepoint(args::value(&values, index));
        let (start, end) = match (read(0), read(1)) {
            (Ok(start), Ok(end)) => (start, end),
            (Err(detail), _) | (_, Err(detail)) => {
                return error::fail(context, SITE, 1, detail, Value::Bool(false));
            }
        };
        let Some(choice) = Choice::read(&values, 3) else {
            error::warning_code(context, 1);
            return Ok(Value::Bool(false));
        };
        let choice_value = Value::Int(args::int(&values, 3).unwrap_or(0));
        let callback = args::value(&values, 2);
        for code in start..end {
            if let Some(name) = lookup(code, choice) {
                context
                    .call(
                        callback,
                        vec![
                            Value::Int(i64::from(code)),
                            choice_value.clone(),
                            Value::from(name),
                        ],
                    )
                    .await?;
            }
        }
        Ok(Value::Bool(true))
    })
}

fn lookup(code: u32, choice: Choice) -> Option<String> {
    match choice {
        Choice::Old => None,
        Choice::Alias => table::explicit(code, true).map(str::to_owned),
        Choice::Unicode => table::unicode(code),
        Choice::Extended => {
            table::unicode(code).or_else(|| Some(format!("<{}-{code:04X}>", extended_label(code))))
        }
    }
}

fn reverse_lookup(name: &str, choice: Choice) -> Option<u32> {
    if choice == Choice::Old {
        return None;
    }
    if let Some((label, code)) = name
        .strip_prefix('<')
        .and_then(|name| name.strip_suffix('>'))
        .and_then(|name| name.rsplit_once('-'))
        && choice == Choice::Extended
    {
        let code = u32::from_str_radix(code, 16).ok()?;
        return (code <= 0x10ffff
            && label.eq_ignore_ascii_case(extended_label(code))
            && table::unicode(code).is_none())
        .then_some(code);
    }
    table::reverse(name, choice == Choice::Alias)
}

fn extended_label(code: u32) -> &'static str {
    if CodePointSetData::new::<props::NoncharacterCodePoint>().contains32(code) {
        return "noncharacter";
    }
    match CodePointMapData::<props::GeneralCategory>::new().get32(code) {
        props::GeneralCategory::Control => "control",
        props::GeneralCategory::PrivateUse => "private use area",
        props::GeneralCategory::Surrogate if code < 0xdc00 => "lead surrogate",
        props::GeneralCategory::Surrogate => "trail surrogate",
        _ => "unassigned",
    }
}
