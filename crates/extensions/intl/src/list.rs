//! PHP 8.5 list formatting uses a locale, type and display width.
//!
//! ```php
//! (new IntlListFormatter('en_US'))->format(['one', 'two', 'three']);
//! // "one, two, and three"
//! ```

use std::rc::Rc;

use icu_list::{
    ListFormatter,
    options::{ListFormatterOptions, ListLength},
};
use phera_extension_api::{ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture};
use phera_extension_api::{MethodEntry, NativeClone, NativeError, NativeResult, Object};
use phera_extension_api::{Param, ParamType, Registry, Value, args};

use crate::{error, locale, locale_id, number};

const CLASS: &str = "IntlListFormatter";
const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<List>()),
    ..ClassHooks::EMPTY
};

#[derive(Clone)]
struct List {
    formatter: Rc<ListFormatter>,
    error: (i32, Option<String>),
}

fn construct(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    const SITE: &str = "IntlListFormatter::__construct";
    error::clear(context);
    let input = locale::effective(context, args::bytes(values, 0));
    let locale = locale_id::cldr(&input);
    if locale.id.language.is_unknown() || !number::known_language(locale.id.language.as_str()) {
        return Err(NativeError::value_error(format!(
            "{SITE}(): Argument #1 ($locale) \"{input}\" is invalid"
        )));
    }
    let width = match args::int(values, 2).unwrap_or(0) {
        0 => ListLength::Wide,
        1 => ListLength::Short,
        2 => ListLength::Narrow,
        _ => {
            return Err(NativeError::value_error(concat!(
                "IntlListFormatter::__construct(): Argument #3 ($width) must be one of ",
                "IntlListFormatter::WIDTH_WIDE, IntlListFormatter::WIDTH_SHORT, or ",
                "IntlListFormatter::WIDTH_NARROW",
            )));
        }
    };
    let options = ListFormatterOptions::default().with_length(width);
    let preferences = locale.into();
    let formatter = match args::int(values, 1).unwrap_or(0) {
        0 => ListFormatter::try_new_and(preferences, options),
        1 => ListFormatter::try_new_or(preferences, options),
        2 => ListFormatter::try_new_unit(preferences, options),
        _ => {
            return Err(NativeError::value_error(concat!(
                "IntlListFormatter::__construct(): Argument #2 ($type) must be one of ",
                "IntlListFormatter::TYPE_AND, IntlListFormatter::TYPE_OR, or ",
                "IntlListFormatter::TYPE_UNITS",
            )));
        }
    }
    .map_err(|_| {
        NativeError::throw(
            "IntlException",
            format!("{SITE}(): list formatter creation failed"),
        )
    })?;
    let object = object.ok_or_else(uninitialized)?;
    let mut payload = object.native_payload_mut().map_err(|_| uninitialized())?;
    if payload.is_some() {
        return Err(NativeError::error(
            "IntlListFormatter object is already constructed",
        ));
    }
    *payload = Some(Box::new(List {
        formatter: Rc::new(formatter),
        error: (0, None),
    }));
    Ok(Value::Null)
}

fn uninitialized() -> NativeError {
    NativeError::error("Found unconstructed IntlListFormatter")
}

fn state(object: Option<&Rc<Object>>) -> Option<List> {
    let payload = object?.native_payload().ok()?;
    payload.as_ref()?.downcast_ref::<List>().cloned()
}

fn format(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const SITE: &str = "IntlListFormatter::format";
        error::clear(context);
        let current = state(object.as_ref()).ok_or_else(uninitialized)?;
        let Some(array) = args::array(&values, 0) else {
            return Ok(Value::from(""));
        };
        let mut strings = Vec::with_capacity(array.len());
        for (_, value) in array.iter() {
            let string = context.stringify(value).await?;
            let Ok(text) = std::str::from_utf8(string.as_bytes()) else {
                let detail = "Error converting string to UTF-16";
                if let Some(object) = &object
                    && let Ok(mut payload) = object.native_payload_mut()
                    && let Some(state) = payload
                        .as_mut()
                        .and_then(|state| state.downcast_mut::<List>())
                {
                    state.error = (10, Some(format!("{SITE}(): {detail}")));
                }
                return error::fail(context, SITE, 10, detail, Value::Bool(false));
            };
            strings.push(text.to_owned());
        }
        if let Some(object) = &object
            && let Ok(mut payload) = object.native_payload_mut()
            && let Some(state) = payload
                .as_mut()
                .and_then(|state| state.downcast_mut::<List>())
        {
            state.error = (0, None);
        }
        Ok(Value::from(
            current
                .formatter
                .format_to_string(strings.iter().map(String::as_str)),
        ))
    })
}

pub fn register(registry: &mut Registry) {
    const INIT: &[Param] = &[
        Param::new("locale", ParamType::String),
        Param::new("type", ParamType::Int)
            .with_constant_default("IntlListFormatter::TYPE_AND", ConstantValue::Int(0)),
        Param::new("width", ParamType::Int)
            .with_constant_default("IntlListFormatter::WIDTH_WIDE", ConstantValue::Int(0)),
    ];
    const ITEMS: &[Param] = &[Param::new("strings", ParamType::Array)];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        constants: &[
            ("TYPE_AND", ConstantValue::Int(0)),
            ("TYPE_OR", ConstantValue::Int(1)),
            ("TYPE_UNITS", ConstantValue::Int(2)),
            ("WIDTH_WIDE", ConstantValue::Int(0)),
            ("WIDTH_SHORT", ConstantValue::Int(1)),
            ("WIDTH_NARROW", ConstantValue::Int(2)),
        ],
        hooks: Some(&HOOKS),
        methods: const {
            &[
                MethodEntry::public("__construct", INIT, construct),
                MethodEntry::public_async("format", ITEMS, format).returning("string|false"),
                MethodEntry::public("getErrorCode", &[], |_, object, _| {
                    Ok(Value::Int(
                        state(object).map_or(0, |list| i64::from(list.error.0)),
                    ))
                })
                .returning("int"),
                MethodEntry::public("getErrorMessage", &[], |_, object, _| {
                    let (code, message) = state(object).map_or((0, None), |list| list.error);
                    let name = error::name(code);
                    Ok(Value::from(message.map_or_else(
                        || name.to_owned(),
                        |text| format!("{text}: {name}"),
                    )))
                })
                .returning("string"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}
