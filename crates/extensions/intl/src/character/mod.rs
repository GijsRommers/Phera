//! Common IntlChar operations use ICU4X Unicode categories and simple case mapping.

mod names;
mod property_names;
mod registration;

use icu_casemap::CaseMapper;
use icu_normalizer::properties::CanonicalCombiningClassMap;
use icu_properties::{CodePointMapData, CodePointSetData, props::*};
use phera_extension_api::{Context, NativeResult, Registry, Value, args};

use crate::{error, number};

const CLASS: &str = "IntlChar";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone, Copy)]
enum Op {
    Ord,
    Chr,
    Lower,
    Upper,
    Title,
    Fold,
    Type,
    Combining,
    DigitValue,
    Digit,
    ForDigit,
    Alnum,
    Alpha,
    Base,
    Blank,
    Control,
    Defined,
    Decimal,
    Graph,
    IdIgnore,
    IdPart,
    IdStart,
    IsoControl,
    JavaIdPart,
    JavaIdStart,
    JavaSpace,
    Lowercase,
    Mirrored,
    Print,
    Punct,
    Space,
    Titlecase,
    UAlphabetic,
    ULowercase,
    Uppercase,
    UUppercase,
    UWhiteSpace,
    Whitespace,
    Xdigit,
    Binary,
    IntProperty,
}

fn codepoint(value: &Value) -> Result<u32, &'static str> {
    const INVALID: &str = "Codepoint out of range";
    match value {
        Value::Int(value) => u32::try_from(*value)
            .ok()
            .filter(|value| *value <= 0x10ffff)
            .ok_or(INVALID),
        Value::String(value) => {
            let text = std::str::from_utf8(value.as_bytes()).map_err(|_| INVALID)?;
            let mut chars = text.chars();
            if let Some(character) = chars.next()
                && chars.next().is_none()
            {
                return Ok(u32::from(character));
            }
            Err(concat!(
                "Passing a UTF-8 character for codepoint requires ",
                "a string which is exactly one UTF-8 codepoint long."
            ))
        }
        _ => Err(INVALID),
    }
}

fn encode(code: u32) -> Value {
    if let Some(character) = char::from_u32(code) {
        return Value::from(character.to_string());
    }
    Value::from(vec![
        (0xe0 | (code >> 12)) as u8,
        (0x80 | ((code >> 6) & 0x3f)) as u8,
        (0x80 | (code & 0x3f)) as u8,
    ])
}

fn invoke(context: &mut dyn Context, values: &[Value], site: &str, op: Op) -> NativeResult {
    error::clear(context);
    if let Op::ForDigit = op {
        let digit = args::int(values, 0).unwrap_or(-1);
        let base = args::int(values, 1).unwrap_or(10);
        let code = if (2..=36).contains(&base) && (0..base).contains(&digit) {
            if digit < 10 { 48 + digit } else { 87 + digit }
        } else {
            0
        };
        return Ok(Value::Int(code));
    }
    let input = args::value(values, 0);
    let code = match codepoint(input) {
        Ok(code) => code,
        Err(detail) => return error::fail(context, site, 1, detail, Value::Null),
    };
    let chr = char::from_u32(code);
    let category = CodePointMapData::<GeneralCategory>::new().get32(code);
    let result = match op {
        Op::Ord => Value::Int(i64::from(code)),
        Op::Chr => encode(code),
        Op::Type => Value::Int(i64::from(category as u8)),
        Op::Combining => Value::Int(i64::from(CanonicalCombiningClassMap::new().get32_u8(code))),
        Op::DigitValue => Value::Int(chr.and_then(number::digit).map_or(-1, i64::from)),
        Op::Digit => {
            let base = args::int(values, 1).unwrap_or(10);
            let value = chr.and_then(|ch| {
                number::digit(ch)
                    .map(i64::from)
                    .or_else(|| radix_letter(ch))
            });
            if (2..=36).contains(&base)
                && let Some(value) = value.filter(|value| *value < base)
            {
                Value::Int(value)
            } else {
                return error::fail(context, site, 1, "Invalid digit", Value::Bool(false));
            }
        }
        Op::Lower | Op::Upper | Op::Title | Op::Fold => {
            let mapper = CaseMapper::new();
            let code = chr.map_or(code, |ch| {
                u32::from(match op {
                    Op::Lower => mapper.simple_lowercase(ch),
                    Op::Upper => mapper.simple_uppercase(ch),
                    Op::Title => mapper.simple_titlecase(ch),
                    _ if args::int(values, 1) == Some(1) => mapper.simple_fold_turkic(ch),
                    _ => mapper.simple_fold(ch),
                })
            });
            if matches!(input, Value::Int(_)) {
                Value::Int(i64::from(code))
            } else {
                encode(code)
            }
        }
        Op::Binary => Value::Bool(binary(code, args::int(values, 1).unwrap_or(-1))),
        Op::IntProperty => Value::Int(match args::int(values, 1).unwrap_or(-1) {
            4098 => i64::from(CanonicalCombiningClassMap::new().get32_u8(code)),
            4101 => i64::from(category as u8),
            property => i64::from(binary(code, property)),
        }),
        _ => Value::Bool(predicate(category, code, op)),
    };
    Ok(result)
}

fn radix_letter(character: char) -> Option<i64> {
    let character = match character {
        'Ａ'..='Ｚ' => (u32::from(character) - u32::from('Ａ') + u32::from('A')) as u8,
        'ａ'..='ｚ' => (u32::from(character) - u32::from('ａ') + u32::from('a')) as u8,
        'A'..='Z' | 'a'..='z' => character as u8,
        _ => return None,
    };
    Some(i64::from(character.to_ascii_lowercase() - b'a' + 10))
}

fn binary(code: u32, property: i64) -> bool {
    match property {
        0 => CodePointSetData::new::<Alphabetic>().contains32(code),
        1 => CodePointSetData::new::<AsciiHexDigit>().contains32(code),
        3 => CodePointSetData::new::<BidiMirrored>().contains32(code),
        13 => CodePointSetData::new::<HexDigit>().contains32(code),
        15 => CodePointSetData::new::<IdContinue>().contains32(code),
        16 => CodePointSetData::new::<IdStart>().contains32(code),
        22 => CodePointSetData::new::<Lowercase>().contains32(code),
        30 => CodePointSetData::new::<Uppercase>().contains32(code),
        31 => CodePointSetData::new::<WhiteSpace>().contains32(code),
        _ => false,
    }
}

fn predicate(category: GeneralCategory, code: u32, op: Op) -> bool {
    use GeneralCategory as Cat;
    let letter = matches!(category as u8, 1..=5);
    let mark = matches!(category as u8, 6..=8);
    let numeric = matches!(category as u8, 9..=11);
    let separator = matches!(category as u8, 12..=14);
    let punctuation = matches!(category as u8, 19..=23 | 28..=29);
    let id_ignorable = matches!(code, 0..=8 | 14..=27 | 127..=159) || category == Cat::Format;
    match op {
        Op::Alnum => letter || category == Cat::DecimalNumber,
        Op::Alpha => letter,
        Op::Base => letter || numeric || matches!(category, Cat::EnclosingMark | Cat::SpacingMark),
        Op::Blank => category == Cat::SpaceSeparator || code == 9,
        Op::Control => matches!(
            category,
            Cat::Control | Cat::Format | Cat::LineSeparator | Cat::ParagraphSeparator
        ),
        Op::Defined => category != Cat::Unassigned,
        Op::Decimal => category == Cat::DecimalNumber,
        Op::Graph => {
            !separator && !matches!(category, Cat::Control | Cat::Surrogate | Cat::Unassigned)
        }
        Op::IdIgnore => id_ignorable,
        Op::IdPart => {
            letter || mark || numeric || category == Cat::ConnectorPunctuation || id_ignorable
        }
        Op::IdStart => letter,
        Op::IsoControl => matches!(code, 0..=31 | 127..=159),
        Op::JavaIdPart => {
            letter
                || mark
                || numeric
                || id_ignorable
                || matches!(category, Cat::CurrencySymbol | Cat::ConnectorPunctuation)
        }
        Op::JavaIdStart => {
            letter || matches!(category, Cat::CurrencySymbol | Cat::ConnectorPunctuation)
        }
        Op::JavaSpace => separator,
        Op::Lowercase => category == Cat::LowercaseLetter,
        Op::Mirrored => binary(code, 3),
        Op::Print => !matches!(
            category,
            Cat::Control | Cat::Format | Cat::Surrogate | Cat::Unassigned
        ),
        Op::Punct => punctuation,
        Op::Space => separator || matches!(code, 9..=13 | 28..=31 | 133),
        Op::Titlecase => category == Cat::TitlecaseLetter,
        Op::UAlphabetic => binary(code, 0),
        Op::ULowercase => binary(code, 22),
        Op::Uppercase => category == Cat::UppercaseLetter,
        Op::UUppercase => binary(code, 30),
        Op::UWhiteSpace => binary(code, 31),
        Op::Whitespace => {
            (separator && !matches!(code, 160 | 8199 | 8239)) || matches!(code, 9..=13 | 28..=31)
        }
        Op::Xdigit => category == Cat::DecimalNumber || binary(code, 13),
        _ => false,
    }
}
