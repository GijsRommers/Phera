//! Transliterator applies named Unicode transforms in order to a UTF-16 range.
//!
//! ```php
//! Transliterator::create('Any-Latin; Latin-ASCII; Lower()')->transliterate('Crème');
//! // "creme"
//! ```

mod registration;
mod transform;

use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray};
use phera_extension_api::{Registry, Value, args};

use crate::error;

use transform::Step;

const CLASS: &str = "Transliterator";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone)]
struct State {
    id: String,
    steps: Vec<Step>,
    error: (i32, Option<String>),
}

fn create(context: &mut dyn Context, values: &[Value], site: &str) -> NativeResult {
    let supplied = String::from_utf8_lossy(args::bytes(values, 0));
    let direction = args::int(values, 1).unwrap_or(0);
    error::clear(context);
    let inverse;
    let id = match direction {
        0 => supplied.as_ref(),
        1 => {
            inverse = inverse_id(&supplied);
            &inverse
        }
        _ => {
            return Err(NativeError::value_error(format!(
                "{site}(): Argument #2 ($direction) must be either Transliterator::FORWARD or \
                 Transliterator::REVERSE"
            )));
        }
    };
    let Some(state) = load(id) else {
        let detail = format!("unable to open ICU transliterator with id \"{supplied}\"");
        return error::fail(context, site, 65569, &detail, Value::Null);
    };
    let mut properties = PhpArray::new();
    properties.insert(ArrayKey::str("id"), Value::from(state.id.as_str()));
    let result = context.create_object(CLASS, properties)?;
    if let Value::Object(object) = &result {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(result)
}

fn load(id: &str) -> Option<State> {
    let id = compound_id(id).split(';').map(str::trim);
    let steps = id.clone().map(Step::load).collect::<Option<Vec<_>>>()?;
    let id = id
        .map(|step| match step.split_once(":]") {
            Some((filter, name)) => format!("{filter}:]{}", name.trim_start()),
            None => step.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(";");
    Some(State {
        id,
        steps,
        error: (0, None),
    })
}

fn compound_id(id: &str) -> &str {
    let id = id.trim();
    id.strip_suffix(';').unwrap_or(id).trim_end()
}

fn inverse_id(id: &str) -> String {
    compound_id(id)
        .split(';')
        .rev()
        .map(|step| {
            let step = step.trim();
            match step.to_ascii_lowercase().as_str() {
                "nfc" => "NFD".to_owned(),
                "nfd" => "NFC".to_owned(),
                "nfkc" => "NFKD".to_owned(),
                "nfkd" => "NFKC".to_owned(),
                "lower" => "Upper".to_owned(),
                "upper" => "Lower".to_owned(),
                _ => step.split_once('-').map_or_else(
                    || step.to_owned(),
                    |(source, target)| format!("{target}-{source}"),
                ),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[derive(Clone, Copy)]
enum Op {
    Inverse,
    Transliterate,
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
    let procedural = object.is_none();
    let (object, id, values) = match object {
        Some(object) => (Some(object), None, values),
        None => (
            args::object(values, 0),
            args::string(values, 0).map(|value| String::from_utf8_lossy(value.as_bytes())),
            values.get(1..).unwrap_or_default(),
        ),
    };
    if let Some(object) = object {
        let mut payload = object.native_payload_mut().map_err(|_| missing())?;
        let state = payload
            .as_deref_mut()
            .and_then(|value| value.downcast_mut::<State>())
            .ok_or_else(missing)?;
        return match op {
            Op::Inverse => create(context, &[Value::from(inverse_id(&state.id))], site),
            Op::Transliterate => apply(context, state, values, site, procedural),
            Op::ErrorCode => Ok(Value::Int(i64::from(state.error.0))),
            Op::ErrorMessage => {
                let name = error::name(state.error.0);
                let message = state
                    .error
                    .1
                    .as_ref()
                    .map_or_else(|| name.to_owned(), |details| format!("{details}: {name}"));
                Ok(Value::from(message))
            }
        };
    }
    if let Some(id) = id {
        if let Some(mut state) = load(&id) {
            return apply(context, &mut state, values, site, true);
        }
        let detail = format!("unable to open ICU transliterator with id \"{id}\"");
        context.warning(format!(
            "{site}(): Could not create transliterator with ID \"{id}\" \
             ({site}(): {detail}: U_INVALID_ID)"
        ));
        return error::fail(context, site, 65569, &detail, Value::Bool(false));
    }
    Err(missing())
}

fn apply(
    context: &mut dyn Context,
    state: &mut State,
    values: &[Value],
    site: &str,
    procedural: bool,
) -> NativeResult {
    error::clear(context);
    state.error = (0, None);
    let start_arg = if procedural { 3 } else { 2 };
    let end_arg = start_arg + 1;
    let start = args::int(values, 1).unwrap_or(0);
    let end = args::int(values, 2).unwrap_or(-1);
    if start < 0 {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #{start_arg} ($start) must be greater than or equal to 0"
        )));
    }
    if end < -1 {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #{end_arg} ($end) must be greater than or equal to -1"
        )));
    }
    if end >= 0 && end < start {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #{start_arg} ($start) must be less than or equal to \
             argument #{end_arg} ($end)"
        )));
    }
    let input = args::bytes(values, 0);
    let Ok(input) = std::str::from_utf8(input) else {
        state.error = (
            10,
            Some(format!("{site}(): Error converting string to UTF-16")),
        );
        return Ok(Value::Bool(false));
    };
    let length = input.encode_utf16().count() as i64;
    let end = if end == -1 { length } else { end };
    if start > length || end > length {
        let detail = format!(
            "{site}(): Neither \"start\" nor the \"end\" arguments can exceed the number of \
             UTF-16 code units (in this case, {length})"
        );
        state.error = (1, Some(detail));
        return Ok(Value::Bool(false));
    }
    let from = byte_offset(input, start);
    let to = byte_offset(input, end);
    let transformed = state
        .steps
        .iter()
        .fold(input[from..to].to_owned(), |text, step| step.apply(text));
    Ok(Value::from(format!(
        "{}{transformed}{}",
        &input[..from],
        &input[to..]
    )))
}

fn byte_offset(input: &str, position: i64) -> usize {
    let mut offset = 0;
    for (index, character) in input.char_indices() {
        if offset >= position {
            return index;
        }
        offset += character.len_utf16() as i64;
    }
    input.len()
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed Transliterator")
}
