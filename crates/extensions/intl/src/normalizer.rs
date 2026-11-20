//! Normalizer and its procedural aliases.
//!
//! ```php
//! Normalizer::isNormalized("e\u{0301}");  // false
//! normalizer_normalize("e\u{0301}");      // "é"
//! Normalizer::getRawDecomposition("Å"); // "Å"
//! ```

use icu_normalizer::properties::{CanonicalDecomposition, Decomposed};
use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use phera_extension_api::{ClassEntry, ConstantValue, Context, FunctionEntry, MethodEntry};
use phera_extension_api::{NativeError, NativeResult, Param, ParamType, Registry, Value, args};

use crate::{error, normalization_data};

#[derive(Clone, Copy)]
enum Form {
    CanonicalDecomposition,
    CompatibilityDecomposition,
    CanonicalComposition,
    CompatibilityComposition,
    CompatibilityCasefold,
}

impl Form {
    fn from_arguments(arguments: &[Value]) -> Option<Self> {
        match args::int(arguments, 1).unwrap_or(16) {
            4 => Some(Self::CanonicalDecomposition),
            8 => Some(Self::CompatibilityDecomposition),
            16 => Some(Self::CanonicalComposition),
            32 => Some(Self::CompatibilityComposition),
            48 => Some(Self::CompatibilityCasefold),
            _ => None,
        }
    }

    fn normalize(self, text: &str) -> String {
        match self {
            Self::CanonicalDecomposition => DecomposingNormalizer::new_nfd()
                .normalize(text)
                .into_owned(),
            Self::CompatibilityDecomposition => DecomposingNormalizer::new_nfkd()
                .normalize(text)
                .into_owned(),
            Self::CanonicalComposition => {
                ComposingNormalizer::new_nfc().normalize(text).into_owned()
            }
            Self::CompatibilityComposition => {
                ComposingNormalizer::new_nfkc().normalize(text).into_owned()
            }
            Self::CompatibilityCasefold => normalization_data::casefold(text),
        }
    }

    fn is_normalized(self, text: &str) -> bool {
        match self {
            Self::CanonicalDecomposition => DecomposingNormalizer::new_nfd().is_normalized(text),
            Self::CompatibilityDecomposition => {
                DecomposingNormalizer::new_nfkd().is_normalized(text)
            }
            Self::CanonicalComposition => ComposingNormalizer::new_nfc().is_normalized(text),
            Self::CompatibilityComposition => ComposingNormalizer::new_nfkc().is_normalized(text),
            Self::CompatibilityCasefold => normalization_data::casefold(text) == text,
        }
    }

    fn raw(self, character: char) -> Option<String> {
        if matches!(self, Self::CompatibilityCasefold)
            && let Some(mapping) = normalization_data::casefold_mapping(character)
        {
            return Some(mapping.to_owned());
        }
        if matches!(
            self,
            Self::CompatibilityComposition | Self::CompatibilityDecomposition
        ) && let Some(mapping) = normalization_data::compatibility(character)
        {
            return Some(mapping.to_owned());
        }
        match CanonicalDecomposition::new().decompose(character) {
            Decomposed::Default => None,
            Decomposed::Singleton(first) => Some(first.to_string()),
            Decomposed::Expansion(first, second) => Some([first, second].iter().collect()),
        }
    }
}

fn normalize(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    checked(context, arguments, site, false)
}

fn is_normalized(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    checked(context, arguments, site, true)
}

fn checked(
    context: &mut dyn Context,
    arguments: &[Value],
    site: &str,
    check_only: bool,
) -> NativeResult {
    error::clear(context);
    let form = Form::from_arguments(arguments).ok_or_else(|| {
        NativeError::value_error(format!(
            "{site}(): Argument #2 ($form) must be a a valid normalization form"
        ))
    })?;
    let input = args::string(arguments, 0).map_or(&[][..], |value| value.as_bytes());
    let Ok(text) = std::str::from_utf8(input) else {
        let detail = if check_only {
            "Error converting string to UTF-16."
        } else {
            "Error converting input string to UTF-16"
        };
        return error::fail(
            context,
            site,
            error::INVALID_CHAR,
            detail,
            Value::Bool(false),
        );
    };
    if check_only {
        Ok(Value::Bool(form.is_normalized(text)))
    } else {
        Ok(Value::from(form.normalize(text)))
    }
}

fn raw(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    let input = args::string(arguments, 0).map_or(&[][..], |value| value.as_bytes());
    let prefix = match std::str::from_utf8(input) {
        Ok(text) => text.len(),
        Err(invalid) => invalid.valid_up_to(),
    };
    let first = input.first().copied().unwrap_or_default();
    let consumed = if prefix > 0 {
        std::str::from_utf8(&input[..prefix])
            .ok()
            .and_then(|text| text.chars().next())
            .map_or(0, char::len_utf8)
    } else {
        invalid_scalar_len(input, first)
    };
    if input.is_empty() || consumed != input.len() {
        return error::fail(
            context,
            site,
            error::ILLEGAL_ARGUMENT,
            "Input string must be exactly one UTF-8 encoded code point long.",
            Value::Null,
        );
    }
    let Some(character) = std::str::from_utf8(input)
        .ok()
        .and_then(|text| text.chars().next())
    else {
        return error::fail(
            context,
            site,
            error::ILLEGAL_ARGUMENT,
            "Code point out of range",
            Value::Null,
        );
    };
    let Some(form) = Form::from_arguments(arguments) else {
        return Ok(Value::from(""));
    };
    Ok(form.raw(character).map_or(Value::Null, Value::from))
}

/// ICU's U8_NEXT consumes a valid prefix of an ill-formed sequence.
fn invalid_scalar_len(input: &[u8], first: u8) -> usize {
    if first == 0 {
        return 0;
    }
    match std::str::from_utf8(input) {
        Err(error) => error.error_len().unwrap_or(input.len()),
        Ok(_) => 1,
    }
}

pub fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("string", ParamType::String),
        Param::new("form", ParamType::Int)
            .with_constant_default("Normalizer::FORM_C", ConstantValue::Int(16)),
    ];
    let functions = [
        (
            FunctionEntry::sync("normalizer_normalize", PARAMETERS, |c, a| {
                normalize(c, a, "normalizer_normalize")
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("normalizer_is_normalized", PARAMETERS, |c, a| {
                is_normalized(c, a, "normalizer_is_normalized")
            }),
            "bool",
        ),
        (
            FunctionEntry::sync("normalizer_get_raw_decomposition", PARAMETERS, |c, a| {
                raw(c, a, "normalizer_get_raw_decomposition")
            }),
            "?string",
        ),
    ];
    for (entry, result) in functions {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
    registry.class(ClassEntry {
        constants: &[
            ("FORM_D", ConstantValue::Int(4)),
            ("NFD", ConstantValue::Int(4)),
            ("FORM_KD", ConstantValue::Int(8)),
            ("NFKD", ConstantValue::Int(8)),
            ("FORM_C", ConstantValue::Int(16)),
            ("NFC", ConstantValue::Int(16)),
            ("FORM_KC", ConstantValue::Int(32)),
            ("NFKC", ConstantValue::Int(32)),
            ("FORM_KC_CF", ConstantValue::Int(48)),
            ("NFKC_CF", ConstantValue::Int(48)),
        ],
        methods: const {
            &[
                MethodEntry::public("normalize", PARAMETERS, |c, _, a| {
                    normalize(c, a, "Normalizer::normalize")
                })
                .static_()
                .tentative_return("string|false"),
                MethodEntry::public("isNormalized", PARAMETERS, |c, _, a| {
                    is_normalized(c, a, "Normalizer::isNormalized")
                })
                .static_()
                .tentative_return("bool"),
                MethodEntry::public("getRawDecomposition", PARAMETERS, |c, _, a| {
                    raw(c, a, "Normalizer::getRawDecomposition")
                })
                .static_()
                .tentative_return("?string"),
            ]
        },
        ..ClassEntry::class("Normalizer")
    });
}
