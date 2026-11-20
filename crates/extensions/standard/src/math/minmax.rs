//! `max` and `min`, of several values or of one array.

use phera_extension_api::value::{smart_strcmp, threeway};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, value,
};

use crate::arguments::given_type;

pub(super) fn register(registry: &mut Registry) {
    registry.register_sync_selector("max", |slots| scalar_window(slots).then_some(max));
    registry.register_sync_selector("min", |slots| scalar_window(slots).then_some(min));
    registry.function_with_native_frame(MAX);
    registry.function_with_native_frame(MIN);
}

fn scalar(value: &Value) -> bool {
    !matches!(
        value,
        Value::Object(_) | Value::Array(_) | Value::Reference(_)
    )
}

fn scalar_window(slots: &[Option<Value>]) -> bool {
    if let [Some(Value::Array(array))] = slots {
        return array.values().all(scalar);
    }
    slots.iter().all(|slot| slot.as_ref().is_some_and(scalar))
}

#[derive(Clone, Copy)]
enum Extremum {
    Minimum,
    Maximum,
}

impl Extremum {
    fn function(self) -> &'static str {
        match self {
            Self::Minimum => "min",
            Self::Maximum => "max",
        }
    }

    /// Whether `candidate` replaces `best`. Comparing with `<` or `>`
    /// rather than an `Ordering` keeps PHP's answers for NAN, with which
    /// both are false.
    fn prefers<T: PartialOrd>(self, candidate: T, best: T) -> bool {
        match self {
            Self::Minimum => candidate < best,
            Self::Maximum => candidate > best,
        }
    }
}

/// `max(mixed $value, mixed ...$values): mixed`, or `max(array $value)`.
///
/// Values are compared as with `<` and `>`; of equal ones, the first wins.
///
/// ```php
/// max(1, "5", 3.5); // "5"
/// max([1, 7, 3]);   // 7
/// max([]);          // ValueError
/// ```
const MAX: FunctionEntry = FunctionEntry::asynchronous(
    "max",
    &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    |context, arguments| extremum_with_objects(context, arguments, Extremum::Maximum),
);

fn max(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    extremum(context, arguments, Extremum::Maximum)
}

/// `min(mixed $value, mixed ...$values): mixed`, or `min(array $value)`.
///
/// ```php
/// min(2, 8, -1); // -1
/// ```
const MIN: FunctionEntry = FunctionEntry::asynchronous(
    "min",
    &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    |context, arguments| extremum_with_objects(context, arguments, Extremum::Minimum),
);

fn min(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    extremum(context, arguments, Extremum::Minimum)
}

fn extremum(context: &mut dyn Context, arguments: &[Value], extremum: Extremum) -> NativeResult {
    let function = extremum.function();
    if let [only] = arguments {
        let Value::Array(array) = only else {
            return Err(NativeError::type_error(format!(
                "{function}(): Argument #1 ($value) must be of type array, {} given",
                given_type(only)
            )));
        };
        return select_scalar_array(context, array.values(), extremum);
    }
    // php-src's min() and max() compare ints and floats directly as long
    // as every value so far was one, before falling back to the generic
    // comparison; the array form always compares generically.
    select(context, arguments.iter(), extremum, Path::Numeric)
}

/// How the next comparison is made. Once a value is neither int nor float,
/// PHP stays on the generic comparison for the rest.
#[derive(Clone, Copy)]
enum Path {
    Numeric,
    Variadic,
    Array,
}

impl Path {
    fn numeric(self, extremum: Extremum, best: &Value, candidate: &Value) -> Option<bool> {
        match (self, best, candidate) {
            (Self::Numeric, Value::Int(best), Value::Int(candidate)) => {
                Some(extremum.prefers(candidate, best))
            }
            (Self::Numeric, _, _) => exact_float(best)
                .zip(exact_float(candidate))
                .map(|(best, candidate)| extremum.prefers(candidate, best)),
            (Self::Array | Self::Variadic, _, _) => None,
        }
    }

    /// The array overload puts the current winner on the left. The
    /// variadic overload puts the candidate on the left.
    fn operands<'a>(&mut self, best: &'a Value, candidate: &'a Value) -> (&'a Value, &'a Value) {
        if matches!(self, Self::Array) {
            (best, candidate)
        } else {
            *self = Self::Variadic;
            (candidate, best)
        }
    }

    fn replace(self, extremum: Extremum, order: i8) -> bool {
        if matches!(self, Self::Array) {
            extremum.prefers(0, order)
        } else {
            extremum.prefers(order, 0)
        }
    }
}

fn empty(extremum: Extremum) -> NativeError {
    NativeError::value_error(format!(
        "{}(): Argument #1 ($value) must contain at least one element",
        extremum.function()
    ))
}

fn select_scalar_array<'a>(
    context: &mut dyn Context,
    mut candidates: impl Iterator<Item = &'a Value>,
    extremum: Extremum,
) -> NativeResult {
    let mut best = candidates.next().ok_or_else(|| empty(extremum))?;
    for candidate in candidates {
        let replace = match (best, candidate) {
            (Value::Int(best), Value::Int(candidate)) => extremum.prefers(candidate, best),
            (Value::Float(best), Value::Float(candidate)) => {
                extremum.prefers(0, threeway(*best, *candidate))
            }
            (Value::String(best), Value::String(candidate)) => {
                extremum.prefers(0, smart_strcmp(best.as_bytes(), candidate.as_bytes()))
            }
            _ => extremum.prefers(0, value::compare(best, candidate, context.diagnostics())?),
        };
        if replace {
            best = candidate;
        }
    }
    Ok(best.clone())
}

fn select<'a>(
    context: &mut dyn Context,
    mut candidates: impl Iterator<Item = &'a Value>,
    extremum: Extremum,
    mut path: Path,
) -> NativeResult {
    let mut best = candidates.next().ok_or_else(|| empty(extremum))?;
    for candidate in candidates {
        let replace = match path.numeric(extremum, best, candidate) {
            Some(replace) => replace,
            None => {
                let (left, right) = path.operands(best, candidate);
                path.replace(
                    extremum,
                    value::compare(left, right, context.diagnostics())?,
                )
            }
        };
        if replace {
            best = candidate;
        }
    }
    Ok(best.try_deref()?.clone())
}

fn extremum_with_objects(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    extremum: Extremum,
) -> LocalFuture<'_, NativeResult> {
    let sync = match arguments.as_slice() {
        [Value::Array(array)] => array.values().all(scalar),
        [_] => true,
        values => values.iter().all(scalar),
    };
    if sync {
        return Box::pin(std::future::ready(self::extremum(
            context, &arguments, extremum,
        )));
    }
    Box::pin(async move {
        let (candidates, path) = match arguments.as_slice() {
            [Value::Array(array)] => {
                return select_with_objects(context, array.values(), extremum, Path::Array).await;
            }
            values => (values.iter(), Path::Numeric),
        };
        select_with_objects(context, candidates, extremum, path).await
    })
}

async fn select_with_objects<'a>(
    context: &mut dyn Context,
    mut candidates: impl Iterator<Item = &'a Value>,
    extremum: Extremum,
    mut path: Path,
) -> NativeResult {
    let mut best = candidates.next().ok_or_else(|| empty(extremum))?;
    for candidate in candidates {
        let replace = match path.numeric(extremum, best, candidate) {
            Some(replace) => replace,
            None => {
                let (left, right) = path.operands(best, candidate);
                let order = context.compare(left, right).await?;
                path.replace(extremum, order)
            }
        };
        if replace {
            best = candidate;
        }
    }
    Ok(best.try_deref()?.clone())
}

/// The value as a float, for an int only if the float is exactly the int
/// (so two large ints are never rounded to equal floats).
fn exact_float(value: &Value) -> Option<f64> {
    match value {
        Value::Float(number) => Some(*number),
        Value::Int(integer) if (*integer as f64) as i128 == i128::from(*integer) => {
            Some(*integer as f64)
        }
        _ => None,
    }
}
