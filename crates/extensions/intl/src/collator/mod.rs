//! Collator compares strings according to the requested language and options.
//!
//! ```php
//! $collator = new Collator("de_DE");
//! $collator->compare("ä", "z"); // -1
//! ```

mod options;
mod registration;
mod sort;

use std::rc::Rc;

use icu_collator::options::CollatorOptions;
use icu_collator::{Collator as UnicodeCollator, CollatorBorrowed, CollatorPreferences};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{error, locale, locale_id, number};

const CLASS: &str = "Collator";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

type This<'a> = Option<&'a Rc<Object>>;

#[derive(Clone)]
struct Collator {
    locale: String,
    formatter: Rc<CollatorBorrowed<'static>>,
    prefs: CollatorPreferences,
    options: CollatorOptions,
    legacy: options::LegacyOptions,
    last_error: (i32, Option<String>),
}

fn construct(
    context: &mut dyn Context,
    object: This<'_>,
    arguments: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    let input = locale::effective(context, args::bytes(arguments, 0));
    let locale = locale_id::cldr(input.split('\0').next().unwrap_or_default());
    let prefs: CollatorPreferences = locale.into();
    let Some(formatter) = UnicodeCollator::try_new(prefs, Default::default()).ok() else {
        if object.is_some() {
            return Err(NativeError::throw(
                "IntlException",
                format!("{site}(): collator creation failed"),
            ));
        }
        return error::fail(context, site, 1, "collator creation failed", Value::Null);
    };
    let legacy = options::LegacyOptions::new(&input);
    let state = Collator {
        locale: input,
        formatter: Rc::new(formatter),
        prefs,
        options: CollatorOptions::default(),
        legacy,
        last_error: (0, None),
    };
    if let Some(object) = object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
        return Ok(Value::Null);
    }
    let result = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &result {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum Operation {
    Compare,
    Sort,
    SortKeys,
    Asort,
    GetAttr,
    SetAttr,
    GetStrength,
    SetStrength,
    Locale,
    ErrorCode,
    ErrorMessage,
    SortKey,
}

fn dispatch(
    context: &mut dyn Context,
    object: This<'_>,
    arguments: &[Value],
    site: &str,
    operation: Operation,
) -> NativeResult {
    let (object, arguments) = match object {
        Some(object) => (Some(object), arguments),
        None => (
            args::object(arguments, 0),
            arguments.get(1..).unwrap_or_default(),
        ),
    };
    let mut payload = object
        .ok_or_else(missing)?
        .native_payload_mut()
        .map_err(|_| missing())?;
    let state = payload
        .as_mut()
        .and_then(|data| data.downcast_mut::<Collator>())
        .ok_or_else(missing)?;
    match operation {
        Operation::ErrorCode => return Ok(Value::Int(i64::from(state.last_error.0))),
        Operation::ErrorMessage => {
            let name = error::name(state.last_error.0);
            return Ok(Value::from(state.last_error.1.as_ref().map_or_else(
                || name.to_owned(),
                |message| format!("{message}: {name}"),
            )));
        }
        _ => {
            state.last_error = (0, None);
            error::clear(context);
        }
    }
    match operation {
        Operation::Compare => {
            let (Ok(left), Ok(right)) = (
                std::str::from_utf8(args::bytes(arguments, 0)),
                std::str::from_utf8(args::bytes(arguments, 1)),
            ) else {
                return failure(
                    context,
                    state,
                    site,
                    10,
                    "Error converting string to UTF-16",
                );
            };
            Ok(Value::Int(state.formatter.compare(left, right) as i64))
        }
        Operation::SortKey => {
            let Ok(input) = std::str::from_utf8(args::bytes(arguments, 0)) else {
                return failure(
                    context,
                    state,
                    site,
                    10,
                    "Error converting string to UTF-16",
                );
            };
            Ok(Value::from(sort::key(&state.formatter, input.as_bytes())))
        }
        Operation::Sort | Operation::SortKeys | Operation::Asort => {
            sort::array(context, state, arguments, site, operation)
        }
        Operation::GetStrength => Ok(Value::Int(options::strength_code(
            state.formatter.resolved_options().strength,
        ))),
        Operation::GetAttr => {
            let attribute = args::int(arguments, 0).unwrap_or(0);
            match options::get(state, attribute) {
                Some(code) => Ok(Value::Int(code)),
                None => failure(context, state, site, 1, "Error getting attribute value"),
            }
        }
        Operation::SetAttr | Operation::SetStrength => {
            let (attribute, value) = if matches!(operation, Operation::SetStrength) {
                (5, args::int(arguments, 0).unwrap_or(2))
            } else {
                (
                    args::int(arguments, 0).unwrap_or(0),
                    args::int(arguments, 1).unwrap_or(-1),
                )
            };
            if options::set(state, attribute, value) || matches!(operation, Operation::SetStrength)
            {
                Ok(Value::Bool(true))
            } else {
                failure(context, state, site, 1, "Error setting attribute value")
            }
        }
        Operation::Locale => {
            let kind = args::int(arguments, 0).unwrap_or(0);
            if matches!(kind, 0 | 1) {
                Ok(Value::from(collation_locale(&state.locale, kind)))
            } else {
                failure(context, state, site, 1, "Error getting locale failed")
            }
        }
        Operation::ErrorCode | Operation::ErrorMessage => Ok(Value::Null),
    }
}

fn collation_locale(locale: &str, kind: i64) -> String {
    let resolved = number::resolve_locale(locale);
    if resolved == "root" {
        return resolved;
    }
    let language = locale_id::LocaleId::parse(locale).language;
    if resolved == "en_US_POSIX" || resolved == "fr_CA" {
        return resolved;
    }
    if kind == 0 && matches!(language.as_str(), "en" | "de" | "nl" | "fr" | "it") {
        "root".to_owned()
    } else if kind == 1 && language != "en" {
        language
    } else {
        resolved
    }
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed Collator")
}

fn failure(
    context: &mut dyn Context,
    state: &mut Collator,
    site: &str,
    code: i32,
    detail: &str,
) -> NativeResult {
    state.last_error = (code, Some(format!("{site}(): {detail}")));
    error::fail(context, site, code, detail, Value::Bool(false))
}
