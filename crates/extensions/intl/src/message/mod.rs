//! ICU MessageFormat 1 bindings for PHP's MessageFormatter.
//!
//! ```php
//! MessageFormatter::formatMessage('en_US', '{n, plural, one {one item} other {# items}}',
//!     ['n' => 2]); // "2 items"
//! ```

mod format;
mod parse;
mod pattern;
mod registration;

use std::rc::Rc;

use icu_locale::Locale;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{error, locale, locale_id};

const CLASS: &str = "MessageFormatter";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone)]
struct State {
    locale: Locale,
    requested: String,
    pattern: String,
    nodes: Vec<pattern::Node>,
    error: (i32, Option<String>),
}

impl State {
    fn new(context: &mut dyn Context, values: &[Value]) -> Option<Self> {
        let requested = locale::effective(context, args::bytes(values, 0));
        let mut state = Self {
            locale: locale_id::cldr(&requested),
            requested,
            pattern: String::new(),
            nodes: Vec::new(),
            error: (0, None),
        };
        state.set_pattern(args::bytes(values, 1))?;
        Some(state)
    }

    fn set_pattern(&mut self, pattern: &[u8]) -> Option<()> {
        let pattern = std::str::from_utf8(pattern).ok()?;
        self.nodes = pattern::parse(pattern)?;
        self.pattern = pattern.to_owned();
        Some(())
    }
}

fn create(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    let Some(state) = State::new(context, values) else {
        let detail = "message formatter creation failed";
        if object.is_some() {
            return Err(NativeError::throw(
                "IntlException",
                format!("{site}(): {detail}"),
            ));
        }
        return error::fail(
            context,
            site,
            pattern_error(args::bytes(values, 1)),
            detail,
            Value::Null,
        );
    };
    if let Some(object) = object {
        let mut payload = object.native_payload_mut().map_err(|_| missing())?;
        if payload.is_some() {
            return Err(NativeError::error(
                "MessageFormatter object is already constructed",
            ));
        }
        *payload = Some(Box::new(state));
        return Ok(Value::Null);
    }
    let result = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &result {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum Op {
    Format,
    Parse,
    SetPattern,
    GetPattern,
    Locale,
    ErrorCode,
    ErrorMessage,
}

fn dispatch(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
    op: Op,
) -> NativeResult {
    let (object, values) = match object {
        Some(object) => (Some(object), values),
        None => (args::object(values, 0), values.get(1..).unwrap_or_default()),
    };
    let mut payload = object
        .ok_or_else(missing)?
        .native_payload_mut()
        .map_err(|_| missing())?;
    let state = payload
        .as_deref_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(missing)?;
    apply(context, state, values, site, op)
}

fn standalone(context: &mut dyn Context, values: &[Value], site: &str, op: Op) -> NativeResult {
    error::clear(context);
    let Some(mut state) = State::new(context, values) else {
        return error::fail(
            context,
            site,
            pattern_error(args::bytes(values, 1)),
            "message formatter creation failed",
            Value::Bool(false),
        );
    };
    apply(
        context,
        &mut state,
        values.get(2..).unwrap_or_default(),
        site,
        op,
    )
}

fn apply(
    context: &mut dyn Context,
    state: &mut State,
    values: &[Value],
    site: &str,
    op: Op,
) -> NativeResult {
    if matches!(op, Op::ErrorCode) {
        return Ok(Value::Int(i64::from(state.error.0)));
    }
    if matches!(op, Op::ErrorMessage) {
        let name = error::name(state.error.0);
        let message = state
            .error
            .1
            .as_ref()
            .map_or_else(|| name.to_owned(), |detail| format!("{detail}: {name}"));
        return Ok(Value::from(message));
    }
    error::clear(context);
    state.error = (0, None);
    match op {
        Op::Format => {
            if let Some(values) = args::array(values, 0)
                && let Some(output) = format::write(context, &state.locale, &state.nodes, values)?
            {
                return Ok(Value::from(output));
            }
            failure(context, state, site, 65804, "Formatting failed")
        }
        Op::Parse => {
            match parse::read(context, &state.locale, &state.nodes, args::bytes(values, 0)) {
                Ok(values) => Ok(Value::from(values)),
                Err(code) => failure(context, state, site, code, "Parsing failed"),
            }
        }
        Op::SetPattern => {
            let input = args::bytes(values, 0);
            if state.set_pattern(input).is_some() {
                Ok(Value::Bool(true))
            } else {
                failure(
                    context,
                    state,
                    site,
                    pattern_error(input),
                    "Error setting pattern",
                )
            }
        }
        Op::GetPattern => Ok(Value::from(state.pattern.as_str())),
        Op::Locale => Ok(Value::from(state.requested.as_str())),
        Op::ErrorCode | Op::ErrorMessage => unreachable!("error getters already returned"),
    }
}

fn failure(
    context: &mut dyn Context,
    state: &mut State,
    site: &str,
    code: i32,
    detail: &str,
) -> NativeResult {
    state.error = (code, Some(format!("{site}(): {detail}")));
    error::fail(context, site, code, detail, Value::Bool(false))
}

fn pattern_error(bytes: &[u8]) -> i32 {
    let Ok(pattern) = std::str::from_utf8(bytes) else {
        return 10;
    };
    if pattern.matches('{').count() > pattern.matches('}').count() {
        65801
    } else {
        65802
    }
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed MessageFormatter")
}
