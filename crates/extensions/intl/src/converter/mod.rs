//! UConverter converts UTF, ISO-8859 and Windows code-page strings.
//!
//! ```php
//! $converter = new UConverter('UTF-16BE', 'UTF-8');
//! $converter->convert('é');          // "\x00\xE9"
//! $converter->convert("\x00\xE9", true); // "é"
//! ```

mod encoding;
mod registration;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, Registry, Value, args,
};

use crate::error;
use encoding::Encoding;

const CLASS: &str = "UConverter";
const AMBIGUOUS: i32 = -122;
const NAME_ERROR: &str = "returned error 4: U_FILE_ACCESS_ERROR";
const SUBSTITUTION_ERROR: &str = "returned error 1: U_ILLEGAL_ARGUMENT_ERROR";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone)]
struct Codec {
    encoding: &'static Encoding,
    substitution: Vec<u8>,
}

impl Codec {
    fn new(encoding: &'static Encoding) -> Self {
        Self {
            encoding,
            substitution: encoding.substitution.clone(),
        }
    }
}

#[derive(Clone)]
struct Converter {
    source: Codec,
    destination: Codec,
    error: (i32, Option<String>),
}

fn construct(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    const SITE: &str = "UConverter::__construct";
    error::clear(context);
    let open = |context: &mut dyn Context, index| {
        let label =
            args::string(values, index).map_or(b"UTF-8".as_slice(), |label| label.as_bytes());
        let (code, detail) = match encoding::find(label) {
            Some((encoding, false)) => return Ok(Codec::new(encoding)),
            Some((encoding, true)) => (AMBIGUOUS, ambiguous_alias(encoding)),
            None => (4, NAME_ERROR.to_owned()),
        };
        error::store(context, SITE, code, &detail);
        Err(NativeError::throw(
            "IntlException",
            format!("{SITE}(): {detail}"),
        ))
    };
    let source = open(context, 1)?;
    let destination = open(context, 0)?;
    let mut payload = object
        .ok_or_else(missing)?
        .native_payload_mut()
        .map_err(|_| missing())?;
    *payload = Some(Box::new(Converter {
        source,
        destination,
        error: (0, None),
    }));
    Ok(Value::Null)
}

fn transcode(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    const SITE: &str = "UConverter::transcode";
    error::clear(context);
    let Some((source, source_warning)) = encoding::find(args::bytes(values, 2)) else {
        return error::fail(
            context,
            SITE,
            4,
            "Error setting encoding: 4 - U_FILE_ACCESS_ERROR",
            Value::Bool(false),
        );
    };
    let Some((destination, destination_warning)) = encoding::find(args::bytes(values, 1)) else {
        return error::fail(
            context,
            SITE,
            4,
            "Error setting encoding: 4 - U_FILE_ACCESS_ERROR",
            Value::Bool(false),
        );
    };
    for (encoding, ambiguous) in [(source, source_warning), (destination, destination_warning)] {
        if ambiguous {
            error::fail(
                context,
                SITE,
                AMBIGUOUS,
                &ambiguous_alias(encoding),
                Value::Null,
            )?;
        }
    }

    let options = args::array(values, 3);
    let mut substitution = None;
    for (key, encoding) in [("from_subst", source), ("to_subst", destination)] {
        if let Some(value) = options.and_then(|options| options.get(&ArrayKey::str(key)))
            && let Some(bytes) = value.deref().as_str_bytes()
        {
            if !encoding.valid_substitution(bytes) {
                return error::fail(context, SITE, 1, SUBSTITUTION_ERROR, Value::Bool(false));
            }
            if key == "to_subst" {
                substitution = Some(bytes.to_vec());
            }
        }
    }
    let bytes = args::bytes(values, 0);
    let substitution = substitution.as_deref().unwrap_or(&destination.substitution);
    Ok(Value::from(encoding::convert(
        bytes,
        source,
        destination,
        substitution,
    )))
}

#[derive(Clone, Copy)]
enum Op {
    Convert,
    Source,
    SourceType,
    SetSource,
    Destination,
    DestinationType,
    SetDestination,
    Substitution,
    SetSubstitution,
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
    let mut payload = object
        .ok_or_else(missing)?
        .native_payload_mut()
        .map_err(|_| missing())?;
    let state = payload
        .as_deref_mut()
        .and_then(|data| data.downcast_mut::<Converter>())
        .ok_or_else(missing)?;
    if matches!(op, Op::ErrorCode) {
        return Ok(Value::Int(i64::from(state.error.0)));
    }
    if matches!(op, Op::ErrorMessage) {
        let suffix = error::name(state.error.0);
        return Ok(Value::from(state.error.1.as_ref().map_or_else(
            || suffix.to_owned(),
            |detail| format!("{detail}: {suffix}"),
        )));
    }
    error::clear(context);
    state.error = (0, None);
    let value = match op {
        Op::Convert => {
            let (source, destination) = if args::bool(values, 1).unwrap_or(false) {
                (&state.destination, &state.source)
            } else {
                (&state.source, &state.destination)
            };
            Value::from(encoding::convert(
                args::bytes(values, 0),
                source.encoding,
                destination.encoding,
                &destination.substitution,
            ))
        }
        Op::Source => Value::from(state.source.encoding.name),
        Op::SourceType => Value::Int(state.source.encoding.kind),
        Op::Destination => Value::from(state.destination.encoding.name),
        Op::DestinationType => Value::Int(state.destination.encoding.kind),
        Op::Substitution => Value::from(state.source.substitution.clone()),
        Op::SetSource | Op::SetDestination => {
            let Some((encoding, ambiguous)) = encoding::find(args::bytes(values, 0)) else {
                return fail(context, state, site, 4, NAME_ERROR);
            };
            if ambiguous {
                error::fail(
                    context,
                    site,
                    AMBIGUOUS,
                    &ambiguous_alias(encoding),
                    Value::Null,
                )?;
            }
            if matches!(op, Op::SetSource) {
                state.source = Codec::new(encoding);
            } else {
                state.destination = Codec::new(encoding);
            }
            Value::Bool(true)
        }
        Op::SetSubstitution => {
            let bytes = args::bytes(values, 0);
            if !state.source.encoding.valid_substitution(bytes)
                || !state.destination.encoding.valid_substitution(bytes)
            {
                return fail(context, state, site, 1, SUBSTITUTION_ERROR);
            }
            state.source.substitution = bytes.to_vec();
            state.destination.substitution = bytes.to_vec();
            Value::Bool(true)
        }
        Op::ErrorCode | Op::ErrorMessage => Value::Null,
    };
    Ok(value)
}

fn fail(
    context: &mut dyn Context,
    state: &mut Converter,
    site: &str,
    code: i32,
    detail: &str,
) -> NativeResult {
    state.error = (code, Some(format!("{site}(): {detail}")));
    error::fail(context, site, code, detail, Value::Bool(false))
}

fn ambiguous_alias(encoding: &Encoding) -> String {
    format!("Ambiguous encoding specified, using {}", encoding.name)
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed UConverter")
}
