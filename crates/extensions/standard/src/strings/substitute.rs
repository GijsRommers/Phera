//! Replacing by position and translating: `substr_replace("Hello", "J", 0,
//! 1)` is `"Jello"`, `strtr("abc", "ab", "xy")` is `"xyc"`.

use phera_extension_api::value::to_int;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Value, args, prepare_call, prepare_single_argument,
};

use super::{buffer, reserve, stringify};
use crate::offsets::clamped_range;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[SUBSTR_REPLACE, STRTR];

/// `substr_replace(array|string $string, array|string $replace,
/// array|int $offset, array|int|null $length = null): string|array`, the
/// part that `$offset` and `$length` select (clamped, as in `substr`)
/// replaced.
///
/// With an array of strings, each array argument gives one value per
/// string, in order and ignoring keys; when one runs out, the replacement is
/// empty, the offset 0 and the length "up to the end".
///
/// ```php
/// substr_replace("Hello", "J", 0, 1);           // "Jello"
/// substr_replace("Hello", "!", -1, 0);          // "Hell!o"
/// substr_replace(["ab", "cd"], "X", [0, 1], 1); // ["Xb", "cX"]
/// ```
const SUBSTR_REPLACE: FunctionEntry = FunctionEntry::asynchronous(
    "substr_replace",
    &[
        Param::new("string", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        // array|int, which a parameter list cannot declare; see below.
        Param::new("offset", ParamType::Mixed),
        Param::new("length", ParamType::Mixed).optional(),
    ],
    substr_replace,
);

/// `$offset` and `$length` as they are prepared when they are not arrays.
const SUBSTR_REPLACE_SCALARS: &[Param] = &[
    Param::new("string", ParamType::Mixed),
    Param::new("replace", ParamType::Mixed),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
];

fn substr_replace(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { replace_substrings(context, &arguments).await })
}

async fn replace_substrings(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    // Arrays aside, `$offset` and `$length` are prepared again as `int` and
    // `?int`, so coercion and errors are PHP's; only the type their messages
    // name needs `array|` added. A null offset is deprecated as PHP words it.
    let mut scalars = arguments.to_vec();
    for index in [2, 3] {
        if matches!(scalars.get(index), Some(Value::Array(_))) {
            scalars[index] = Value::Int(0);
        }
    }
    if !context.strict_types() && scalars[2].is_null() {
        context.deprecated(
            "substr_replace(): Passing null to parameter #3 ($offset) of type array|int is deprecated"
                .to_string(),
        );
        scalars[2] = Value::Int(0);
    }
    if let Err(error) = prepare_call(
        "substr_replace",
        SUBSTR_REPLACE_SCALARS,
        &mut scalars,
        context,
    ) {
        return Err(match error {
            NativeError::Throw {
                class,
                message,
                code,
            } => NativeError::Throw {
                class,
                code,
                message: message
                    .replace("of type ?int", "of type array|int|null")
                    .replace("of type int,", "of type array|int,"),
            },
            other => other,
        });
    }
    let Some(strings) = args::array(arguments, 0) else {
        for (index, name) in [(2, "offset"), (3, "length")] {
            if args::array(arguments, index).is_some() {
                return Err(NativeError::type_error(format!(
                    "substr_replace(): Argument #{} (${name}) cannot be an array when working on a single string",
                    index + 1
                )));
            }
        }
        return replace_part(context, &arguments[0], 0, arguments, &scalars).await;
    };
    let mut result = PhpArray::new();
    for (position, (key, string)) in strings.iter().enumerate() {
        result.insert(
            key,
            replace_part(context, string, position, arguments, &scalars).await?,
        );
    }
    Ok(Value::from(result))
}

/// `substr_replace` for the string at `position`.
async fn replace_part(
    context: &mut dyn Context,
    string: &Value,
    position: usize,
    arguments: &[Value],
    scalars: &[Value],
) -> NativeResult {
    // An array argument gives its value at this position, others give
    // themselves.
    let nth = |index: usize| match args::array(arguments, index) {
        Some(array) => array.values().nth(position),
        None => scalars.get(index),
    };
    let string = stringify(context, string).await?;
    let offset = to_int(nth(2).unwrap_or(&Value::Int(0)), context.diagnostics())?;
    let length = match nth(3) {
        None | Some(Value::Null) => None,
        Some(length) => Some(to_int(length, context.diagnostics())?),
    };
    let replacement = stringify(context, nth(1).unwrap_or(&Value::Null)).await?;
    let range = clamped_range(string.len(), offset, length);
    let mut result = buffer(string.len() - range.len() + replacement.len())?;
    result.extend_from_slice(&string[..range.start]);
    result.extend_from_slice(replacement.as_bytes());
    result.extend_from_slice(&string[range.end..]);
    Ok(Value::from(result))
}

/// `strtr(string $string, string $from, string $to): string` and
/// `strtr(string $string, array $replace_pairs): string`.
///
/// The first form maps each byte of `$from` to the byte at the same place
/// in `$to` (extra bytes in either are ignored). The second replaces whole
/// strings, trying the longest key first at each position and never looking
/// at a replacement again; that is how it differs from `str_replace`.
///
/// ```php
/// strtr("abc", "ab", "xy");                   // "xyc"
/// strtr("Hi all", ["Hi" => "Hello", "Hello" => "x"]); // "Hello all"
/// ```
const STRTR: FunctionEntry = FunctionEntry::asynchronous(
    "strtr",
    &[
        Param::new("string", ParamType::String),
        // string|array, depending on the form; see below.
        Param::new("from", ParamType::Mixed),
        Param::new("to", ParamType::Mixed).optional(),
    ],
    strtr,
);

/// The parameters of each form, which decide how the arguments are prepared.
const STRTR_PAIRS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("from", ParamType::Array),
];
const STRTR_BYTES: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("from", ParamType::String),
    Param::new("to", ParamType::String),
];

fn strtr(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(translate(context, arguments))
}

async fn translate(context: &mut dyn Context, mut prepared: Vec<Value>) -> NativeResult {
    let params = if prepared.len() == 2 {
        STRTR_PAIRS
    } else {
        STRTR_BYTES
    };
    if prepared.len() == STRTR_BYTES.len() && !context.strict_types() {
        // The byte translation form declares two string parameters that
        // are mixed at the outer native boundary shared with the array form.
        for (index, argument) in prepared.iter_mut().enumerate().skip(1) {
            let stringable = matches!(&*argument.deref(), Value::Object(object)
                if object.class.is_instance_of("Stringable"));
            if stringable {
                let text = context.stringify(argument).await?;
                *argument = Value::String(text);
            } else {
                prepare_single_argument("strtr", index, &params[index], argument, context)?;
            }
        }
    }
    prepare_call("strtr", params, &mut prepared, context)?;
    let string = args::bytes(&prepared, 0);
    if string.is_empty() {
        return Ok(Value::from(""));
    }
    if let Some(pairs) = args::array(&prepared, 1) {
        return translate_pairs(context, string, pairs).await;
    }
    let mut table: [u8; 256] = std::array::from_fn(|byte| byte as u8);
    for (&from, &to) in args::bytes(&prepared, 1)
        .iter()
        .zip(args::bytes(&prepared, 2))
    {
        table[usize::from(from)] = to;
    }
    let translated: Vec<u8> = string
        .iter()
        .map(|&byte| table[usize::from(byte)])
        .collect();
    Ok(Value::from(translated))
}

async fn translate_pairs(
    context: &mut dyn Context,
    string: &[u8],
    pairs: &PhpArray,
) -> NativeResult {
    // With a single pair, PHP converts the replacement to a string at once,
    // so its diagnostics come even when the key never matches.
    let single_replacement = match pairs.values().next() {
        Some(value) if pairs.len() == 1 => Some(Value::String(stringify(context, value).await?)),
        _ => None,
    };
    let mut patterns = Vec::new();
    for (key, value) in pairs.iter() {
        let key = match key {
            ArrayKey::Int(number) => number.to_string().into_bytes(),
            ArrayKey::String(string) => string.as_bytes().to_vec(),
        };
        if key.is_empty() {
            context.warning("strtr(): Ignoring replacement of empty string".to_string());
            continue;
        }
        patterns.push((key, single_replacement.as_ref().unwrap_or(value)));
    }
    // A stable sort, so keys of the same length keep their order.
    patterns.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
    let mut result = buffer(string.len())?;
    let mut rest = string;
    while let Some(&byte) = rest.first() {
        if let Some((key, replacement)) = patterns.iter().find(|(key, _)| rest.starts_with(key)) {
            let replacement = stringify(context, replacement).await?;
            reserve(&mut result, replacement.len())?;
            result.extend_from_slice(replacement.as_bytes());
            rest = &rest[key.len()..];
        } else {
            reserve(&mut result, 1)?;
            result.push(byte);
            rest = &rest[1..];
        }
    }
    Ok(Value::from(result))
}
