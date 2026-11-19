//! UTS #46 domain names preserve PHP's output flags and by-reference details.
//!
//! ```php
//! idn_to_ascii('faß.de', IDNA_NONTRANSITIONAL_TO_ASCII); // "xn--fa-hia.de"
//! ```

use icu_normalizer::{ComposingNormalizer, uts46::Uts46Mapper};
use icu_properties::{CodePointMapData, props::GeneralCategory};
use phera_extension_api::{ArrayKey, ConstantValue, Context, FunctionEntry, NativeError};
use phera_extension_api::{NativeResult, Param, ParamType, PhpArray, Registry, Value, args};

use crate::{error, idn_validation};

const DEFAULT_FLAGS: i64 = 48;
const EMPTY_LABEL: i64 = 1;
const LABEL_TOO_LONG: i64 = 2;
const DOMAIN_TOO_LONG: i64 = 4;
const LEADING_HYPHEN: i64 = 8;
const TRAILING_HYPHEN: i64 = 16;
const HYPHEN_3_4: i64 = 32;
const LEADING_COMBINING_MARK: i64 = 64;
const DISALLOWED: i64 = 128;
const PUNYCODE: i64 = 256;
const LABEL_HAS_DOT: i64 = 512;
const INVALID_ACE_LABEL: i64 = 1024;
const BIDI: i64 = 2048;
const CONTEXTJ: i64 = 4096;

#[derive(Clone, Copy)]
enum Mode {
    Ascii,
    Unicode,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Ascii => "idn_to_ascii",
            Self::Unicode => "idn_to_utf8",
        }
    }

    fn capacity(self) -> usize {
        match self {
            Self::Ascii => 255,
            Self::Unicode => 1008,
        }
    }
}

struct Options {
    std3: bool,
    bidi: bool,
    context_j: bool,
    transitional: bool,
}

impl Options {
    fn new(flags: i64, mode: Mode) -> Self {
        let non_transitional = match mode {
            Mode::Ascii => 16,
            Mode::Unicode => 32,
        };
        Self {
            std3: flags & 2 != 0,
            bidi: flags & 4 != 0,
            context_j: flags & 8 != 0,
            transitional: flags & non_transitional == 0,
        }
    }
}

struct Label {
    text: String,
    errors: i64,
}

fn convert(context: &mut dyn Context, values: &[Value], mode: Mode) -> NativeResult {
    error::clear(context);
    let site = mode.name();
    let input = args::bytes(values, 0);
    if input.is_empty() {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($domain) must not be empty"
        )));
    }
    if input.len() >= i32::MAX as usize {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($domain) must be less than 2147483647 bytes"
        )));
    }
    if args::int(values, 2).unwrap_or(1) != 1 {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #2 ($flags) must be INTL_IDNA_VARIANT_UTS46"
        )));
    }
    let reference = args::reference(values, 3)
        .map(|cell| cell.try_borrow_mut())
        .transpose()
        .map_err(|_| {
            NativeError::error(format!("{site}(): Argument #4 is already being modified"))
        })?;
    let mut info = PhpArray::new();
    let flags = Options::new(args::int(values, 1).unwrap_or(DEFAULT_FLAGS), mode);
    let domain = String::from_utf8_lossy(input);
    let output = process(&domain, flags, mode).map_or(Value::Bool(false), |processed| {
        info.insert(ArrayKey::str("result"), Value::from(processed.text.clone()));
        info.insert(
            ArrayKey::str("isTransitionalDifferent"),
            Value::Bool(processed.deviation),
        );
        info.insert(ArrayKey::str("errors"), Value::Int(processed.errors));
        if processed.errors == 0 {
            Value::from(processed.text)
        } else {
            Value::Bool(false)
        }
    });
    if let Some(mut reference) = reference {
        *reference = Value::from(info);
    }
    Ok(output)
}

struct Processed {
    text: String,
    errors: i64,
    deviation: bool,
}

fn process(domain: &str, flags: Options, mode: Mode) -> Option<Processed> {
    let mapper = Uts46Mapper::new();
    let mapped: String = mapper.map_normalize(domain.chars()).collect();
    let deviation = mapped
        .chars()
        .any(|c| matches!(c, 'ß' | 'ς' | '\u{200c}' | '\u{200d}'));
    let mapped = if deviation && flags.transitional {
        let mut transitioned = String::with_capacity(mapped.len());
        for scalar in mapped.chars() {
            match scalar {
                'ß' => transitioned.push_str("ss"),
                'ς' => transitioned.push('σ'),
                '\u{200c}' | '\u{200d}' => {}
                other => transitioned.push(other),
            }
        }
        ComposingNormalizer::new_nfc()
            .normalize(&transitioned)
            .into_owned()
    } else {
        mapped
    };
    // A label longer than the output buffer cannot fit once encoded either.
    if mapped.len() > mode.capacity() * 4 {
        return None;
    }
    let mut labels: Vec<_> = mapped
        .split('.')
        .map(|text| decode(text, flags.std3))
        .collect();
    let root_dot = mapped.ends_with('.');
    if root_dot && let Some(last) = labels.last_mut() {
        last.errors = 0;
    }
    let bidi = flags.bidi
        && labels
            .iter()
            .any(|label| idn_validation::has_rtl(&label.text));
    let mut text = String::new();
    let mut errors = 0;
    for (index, label) in labels.into_iter().enumerate() {
        if index > 0 {
            text.push('.');
        }
        errors |= label.errors;
        let invalid = label.errors
            & (DISALLOWED | LABEL_HAS_DOT | LEADING_COMBINING_MARK | PUNYCODE | INVALID_ACE_LABEL)
            != 0;
        if !invalid {
            if flags.context_j && !idn_validation::valid_context_j(&label.text) {
                errors |= CONTEXTJ;
            }
            if bidi && !label.text.is_empty() && !idn_validation::valid_bidi(&label.text) {
                errors |= BIDI;
            }
        }
        let rendered = match mode {
            Mode::Ascii if !invalid && !label.text.is_ascii() => {
                if label.text.chars().take(mode.capacity()).count() >= mode.capacity() {
                    return None;
                }
                format!("xn--{}", idna::punycode::encode_str(&label.text)?)
            }
            _ => label.text,
        };
        if matches!(mode, Mode::Ascii) && !invalid && rendered.len() > 63 {
            errors |= LABEL_TOO_LONG;
        }
        text.push_str(&rendered);
    }
    if text.len() >= mode.capacity() {
        return None;
    }
    if matches!(mode, Mode::Ascii) && text.len() > 253 + usize::from(root_dot) {
        errors |= DOMAIN_TOO_LONG;
    }
    Some(Processed {
        text,
        errors,
        deviation,
    })
}

fn decode(label: &str, std3: bool) -> Label {
    if let Some(payload) = label.strip_prefix("xn--") {
        let Some(decoded) = idna::punycode::decode_to_string(payload) else {
            return Label {
                text: label.to_owned(),
                errors: PUNYCODE,
            };
        };
        let validated = Uts46Mapper::new()
            .normalize_validate(decoded.chars())
            .eq(decoded.chars());
        let decoded = validate(decoded, std3);
        if !validated
            || decoded.text.is_ascii()
            || payload.ends_with('-')
            || decoded.errors & (DISALLOWED | LEADING_COMBINING_MARK | LABEL_HAS_DOT) != 0
        {
            return Label {
                text: format!("{label}�"),
                errors: INVALID_ACE_LABEL,
            };
        }
        return decoded;
    }
    validate(label.to_owned(), std3)
}

fn validate(mut label: String, std3: bool) -> Label {
    let mut errors = 0;
    if label.is_empty() {
        errors |= EMPTY_LABEL;
    }
    if label.starts_with('-') {
        errors |= LEADING_HYPHEN;
    }
    if label.ends_with('-') {
        errors |= TRAILING_HYPHEN;
    }
    if label.chars().skip(2).take(2).eq(['-', '-']) {
        errors |= HYPHEN_3_4;
    }
    if let Some(first) = label.chars().next()
        && matches!(
            CodePointMapData::<GeneralCategory>::new().get(first),
            GeneralCategory::NonspacingMark
                | GeneralCategory::SpacingMark
                | GeneralCategory::EnclosingMark
        )
    {
        label.replace_range(..first.len_utf8(), "�");
        errors |= LEADING_COMBINING_MARK;
    }
    if label.contains('.') {
        label = label.replace('.', "�");
        errors |= LABEL_HAS_DOT;
    }
    if label.contains('�') && errors & (LEADING_COMBINING_MARK | LABEL_HAS_DOT) == 0 {
        errors |= DISALLOWED;
    }
    if std3 && label.bytes().any(forbidden_ascii) {
        label = label
            .chars()
            .map(|scalar| {
                if scalar.is_ascii() && forbidden_ascii(scalar as u8) {
                    '�'
                } else {
                    scalar
                }
            })
            .collect();
        errors |= DISALLOWED;
    }
    Label {
        text: label,
        errors,
    }
}

fn forbidden_ascii(byte: u8) -> bool {
    byte.is_ascii() && !byte.is_ascii_alphanumeric() && byte != b'-'
}

pub fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("domain", ParamType::String),
        Param::new("flags", ParamType::Int)
            .with_constant_default("IDNA_DEFAULT", ConstantValue::Int(DEFAULT_FLAGS)),
        Param::new("variant", ParamType::Int)
            .with_constant_default("INTL_IDNA_VARIANT_UTS46", ConstantValue::Int(1)),
        Param::new("idna_info", ParamType::Mixed)
            .by_ref()
            .with_default(ConstantValue::Null),
    ];
    registry.function(FunctionEntry::sync("idn_to_ascii", PARAMETERS, |c, a| {
        convert(c, a, Mode::Ascii)
    }));
    registry.function(FunctionEntry::sync("idn_to_utf8", PARAMETERS, |c, a| {
        convert(c, a, Mode::Unicode)
    }));
    for name in ["idn_to_ascii", "idn_to_utf8"] {
        registry.describe_function(name, "string|false", None);
    }
}
