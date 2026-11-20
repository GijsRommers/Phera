//! Locale queries use the active request's intl.default_locale for an empty ID.
//!
//! ```php
//! Locale::setDefault("nl_BE");
//! Locale::getRegion(""); // "BE"
//! ```

use std::sync::OnceLock;

use icu_locale::LocaleDirectionality;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::locale_id::{self, LocaleId};
use crate::{error, locale_display, locale_matching};

mod registration;

pub const MAX_LEN: usize = 156;
const INI: &str = "intl.default_locale";

pub fn register(registry: &mut Registry) {
    registry.ini_update(INI, |_, _, value, _| {
        Ok(value.is_none_or(|value| !value.is_empty()))
    });
    registration::register(registry);
}

pub fn default_locale(context: &dyn Context) -> String {
    context
        .ini_get(INI.as_bytes())
        .filter(|value| !value.as_bytes().is_empty())
        .map_or_else(
            || process_locale().to_owned(),
            |value| String::from_utf8_lossy(value.as_bytes()).into_owned(),
        )
}

fn process_locale() -> &'static str {
    static LOCALE: OnceLock<String> = OnceLock::new();
    LOCALE.get_or_init(|| {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        let base = locale.split('.').next().unwrap_or_default();
        match base {
            "" | "C" | "POSIX" => "en_US_POSIX".to_owned(),
            _ => locale_id::canonical(&locale),
        }
    })
}

pub fn check_null(
    arguments: &[Value],
    index: usize,
    parameter: &str,
    site: &str,
) -> Result<(), NativeError> {
    if args::bytes(arguments, index).contains(&0) {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #{} (${parameter}) must not contain any null bytes",
            index + 1
        )));
    }
    Ok(())
}

pub fn effective(context: &dyn Context, value: &[u8]) -> String {
    if value.is_empty() {
        default_locale(context)
    } else {
        String::from_utf8_lossy(value).into_owned()
    }
}

pub fn too_long(context: &mut dyn Context, site: &str) -> NativeResult {
    too_long_for(context, site, false)
}

pub fn too_long_for(context: &mut dyn Context, site: &str, constructor: bool) -> NativeResult {
    const DETAIL: &str = "Locale string too long, should be no longer than 156 characters";
    let result = error::fail(
        context,
        site,
        error::ILLEGAL_ARGUMENT,
        DETAIL,
        Value::Null,
    );
    if constructor {
        Err(NativeError::throw("IntlException", format!("{site}(): {DETAIL}")))
    } else {
        result
    }
}

fn get_default(context: &mut dyn Context, _: &[Value], _: &str) -> NativeResult {
    Ok(Value::from(default_locale(context)))
}

fn set_default(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    check_null(arguments, 0, "locale", site)?;
    let value = args::string(arguments, 0).cloned().unwrap_or_default();
    let value = if value.as_bytes().is_empty() {
        process_locale().into()
    } else {
        value
    };
    context.ini_set(site, INI.as_bytes(), value)?;
    Ok(Value::Bool(true))
}

enum Query {
    Language,
    Script,
    Region,
    Variants,
    Parse,
    Canonicalize,
    Keywords,
    RightToLeft,
    LikelySubtags,
    Minimize,
}

fn query(context: &mut dyn Context, arguments: &[Value], site: &str, query: Query) -> NativeResult {
    if !matches!(
        query,
        Query::RightToLeft | Query::LikelySubtags | Query::Minimize
    ) {
        error::clear(context);
    }
    check_null(arguments, 0, "locale", site)?;
    let input = effective(context, args::bytes(arguments, 0));
    if input.len() > MAX_LEN && !matches!(query, Query::RightToLeft) {
        return if matches!(query, Query::LikelySubtags | Query::Minimize) {
            error::fail(
                context,
                site,
                error::ILLEGAL_ARGUMENT,
                "invalid locale",
                Value::Bool(false),
            )
        } else {
            too_long(context, site)
        };
    }
    let id = LocaleId::parse(&input);
    let value = match query {
        Query::Language => {
            if id.language.is_empty() && !id.private.is_empty() && input.starts_with(['x', 'X']) {
                Value::from(format!("x-{}", id.private[0].to_ascii_lowercase()))
            } else {
                Value::from(id.language)
            }
        }
        Query::Script if id.grandfathered.is_some() => {
            return error::fail(context, site, 0, "unable to get locale script", Value::Null);
        }
        Query::Region if id.grandfathered.is_some() => {
            return error::fail(context, site, 0, "unable to get locale region", Value::Null);
        }
        Query::Script => Value::from(id.script),
        Query::Region if id.language.is_empty() && input.starts_with(['x', 'X']) => Value::from(
            id.private
                .get(1)
                .filter(|part| matches!(part.len(), 2 | 3))
                .map_or_else(String::new, |part| part.to_ascii_uppercase()),
        ),
        Query::Region => Value::from(id.region),
        Query::Variants => array(
            locale_id::variants(&input, &id)
                .into_iter()
                .enumerate()
                .map(|(index, variant)| (ArrayKey::Int(index as i64), Value::from(variant))),
        ),
        Query::Parse => parse(id),
        Query::Canonicalize => Value::from(locale_id::canonical(&input)),
        Query::Keywords => {
            let fields = locale_id::keywords(&input);
            if fields.is_empty() {
                Value::Null
            } else {
                array(
                    fields
                        .into_iter()
                        .map(|(key, value)| (ArrayKey::str(key), Value::from(value))),
                )
            }
        }
        Query::RightToLeft => Value::Bool(
            LocaleDirectionality::new_extended().is_right_to_left(&locale_id::cldr(&input).id),
        ),
        Query::LikelySubtags => Value::from(locale_id::change_likely(&input, false)),
        Query::Minimize => Value::from(locale_id::change_likely(&input, true)),
    };
    Ok(value)
}

fn parse(id: LocaleId<'_>) -> Value {
    if let Some(tag) = id.grandfathered {
        return array([(ArrayKey::str("grandfathered"), Value::from(tag))]);
    }
    let core = [
        ("language", id.language),
        ("script", id.script),
        ("region", id.region),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| (ArrayKey::str(key), Value::from(value)));
    let variants = id
        .variants
        .into_iter()
        .enumerate()
        .map(|(index, value)| (ArrayKey::str(format!("variant{index}")), Value::from(value)));
    let private = id
        .private
        .into_iter()
        .enumerate()
        .map(|(index, value)| (ArrayKey::str(format!("private{index}")), Value::from(value)));
    array(core.chain(variants).chain(private))
}

fn array(fields: impl IntoIterator<Item = (ArrayKey, Value)>) -> Value {
    Value::from(fields.into_iter().collect::<PhpArray>())
}

fn compose(context: &mut dyn Context, arguments: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    let Some(fields) = args::array(arguments, 0).filter(|array| !array.is_empty()) else {
        return Ok(Value::Bool(false));
    };
    if let Some(value) = fields.get(&ArrayKey::str("grandfathered")) {
        return match &*value.deref() {
            Value::String(value) => Ok(Value::from(value.clone())),
            _ => invalid_field(context, site),
        };
    }
    let Some(language) = fields.get(&ArrayKey::str("language")) else {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($subtags) must contain a \"language\" key"
        )));
    };
    let Value::String(language) = &*language.deref() else {
        return invalid_field(context, site);
    };
    let mut result = language.as_bytes().to_vec();
    for (field, groups, prefix) in [
        ("extlang", 3, Some("a")),
        ("script", 0, None),
        ("region", 0, None),
        ("variant", 15, None),
        ("private", 15, Some("x")),
    ] {
        if append(fields, &mut result, field, groups, prefix).is_none() {
            return invalid_field(context, site);
        }
    }
    Ok(Value::from(result))
}

fn append(
    fields: &PhpArray,
    output: &mut Vec<u8>,
    field: &str,
    groups: usize,
    prefix: Option<&str>,
) -> Option<()> {
    let key = ArrayKey::str(field);
    let mut values = Vec::new();
    if let Some(value) = fields.get(&key) {
        match &*value.deref() {
            Value::String(string) if groups == 0 => values.push(string.clone()),
            Value::Array(items) if groups > 0 => {
                for (_, value) in items.iter() {
                    let Value::String(string) = &*value.deref() else {
                        return None;
                    };
                    values.push(string.clone());
                }
            }
            _ => return None,
        }
    } else {
        for index in 0..groups {
            if let Some(value) = fields.get(&ArrayKey::str(format!("{field}{index}"))) {
                let Value::String(string) = &*value.deref() else {
                    return None;
                };
                values.push(string.clone());
            }
        }
    }
    if !values.is_empty()
        && let Some(prefix) = prefix
    {
        output.push(b'_');
        output.extend_from_slice(prefix.as_bytes());
    }
    for value in values {
        output.push(b'_');
        output.extend_from_slice(value.as_bytes());
    }
    Some(())
}

fn invalid_field(context: &mut dyn Context, site: &str) -> NativeResult {
    error::fail(
        context,
        site,
        error::ILLEGAL_ARGUMENT,
        "parameter array element is not a string",
        Value::Bool(false),
    )
}
