//! `max` and `min`, of several values or of one array.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, value,
};

use crate::arguments::given_type;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[MAX, MIN];

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
const MAX: FunctionEntry = FunctionEntry::sync(
    "max",
    &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    max,
);

fn max(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    extremum(context, arguments, Extremum::Maximum)
}

/// `min(mixed $value, mixed ...$values): mixed`, or `min(array $value)`.
///
/// ```php
/// min(2, 8, -1); // -1
/// ```
const MIN: FunctionEntry = FunctionEntry::sync(
    "min",
    &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    min,
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
        return select(context, array.values(), extremum, Path::Array);
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

fn select<'a>(
    context: &mut dyn Context,
    mut candidates: impl Iterator<Item = &'a Value>,
    extremum: Extremum,
    mut path: Path,
) -> NativeResult {
    let Some(mut best) = candidates.next() else {
        return Err(NativeError::value_error(format!(
            "{}(): Argument #1 ($value) must contain at least one element",
            extremum.function()
        )));
    };
    for candidate in candidates {
        let numeric = match (path, best, candidate) {
            (Path::Numeric, Value::Int(best), Value::Int(candidate)) => {
                Some(extremum.prefers(candidate, best))
            }
            (Path::Numeric, _, _) => exact_float(best)
                .zip(exact_float(candidate))
                .map(|(best, candidate)| extremum.prefers(candidate, best)),
            (Path::Array | Path::Variadic, _, _) => None,
        };
        let replace = match numeric {
            Some(replace) => replace,
            None => match path {
                // The array overload puts the current winner on the left.
                Path::Array => {
                    extremum.prefers(0, value::compare(best, candidate, context.diagnostics())?)
                }
                Path::Numeric | Path::Variadic => {
                    path = Path::Variadic;
                    extremum.prefers(value::compare(candidate, best, context.diagnostics())?, 0)
                }
            },
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
