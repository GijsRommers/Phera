//! Minimum and maximum selection with PHP's sticky numeric fast path.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, value,
};

#[derive(Clone, Copy)]
enum Extremum {
    Minimum,
    Maximum,
}

impl Extremum {
    fn name(self) -> &'static str {
        match self {
            Self::Minimum => "min",
            Self::Maximum => "max",
        }
    }

    /// Native comparisons preserve PHP's special handling of NaN in variadic calls.
    fn prefers<T: PartialOrd>(self, candidate: T, current: T) -> bool {
        match self {
            Self::Minimum => candidate < current,
            Self::Maximum => candidate > current,
        }
    }
}

enum ComparisonPath {
    Numeric,
    Generic,
}

const MINMAX_PARAMS: &[Param] = &[
    Param::new("value", ParamType::Mixed),
    Param::new("values", ParamType::Mixed).variadic(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("max", MINMAX_PARAMS, max));
    registry.function(FunctionEntry::sync("min", MINMAX_PARAMS, min));
}

fn max(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    select(context, arguments, Extremum::Maximum)
}

fn min(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    select(context, arguments, Extremum::Minimum)
}

/// The one-argument overload requires an array; variadic calls accept any values.
fn candidates<'a>(arguments: &'a [Value], name: &str) -> Result<Vec<&'a Value>, NativeError> {
    if arguments.len() != 1 {
        return Ok(arguments.iter().collect());
    }
    match arguments.first() {
        Some(Value::Array(array)) => Ok(array.values().collect()),
        Some(value) => Err(NativeError::type_error(format!(
            "{name}(): Argument #1 ($value) must be of type array, {} given",
            crate::argument_type_name(value)
        ))),
        None => Ok(Vec::new()),
    }
}

/// Once a nonnumeric value appears, PHP never returns to native numeric comparison.
fn select(context: &mut dyn Context, arguments: &[Value], extremum: Extremum) -> NativeResult {
    let name = extremum.name();
    let mut candidates = candidates(arguments, name)?.into_iter();
    let Some(mut best) = candidates.next() else {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #1 ($value) must contain at least one element"
        )));
    };
    let mut path = if arguments.len() == 1 {
        ComparisonPath::Generic
    } else {
        ComparisonPath::Numeric
    };
    for candidate in candidates {
        let numeric = match (&path, best, candidate) {
            (ComparisonPath::Numeric, Value::Int(current), Value::Int(candidate)) => {
                Some(extremum.prefers(candidate, current))
            }
            (ComparisonPath::Numeric, _, _) => exact_number(best)
                .zip(exact_number(candidate))
                .map(|(current, candidate)| extremum.prefers(candidate, current)),
            _ => None,
        };
        let replace = if let Some(result) = numeric {
            result
        } else {
            path = ComparisonPath::Generic;
            extremum.prefers(value::compare(candidate, best, context.diagnostics())?, 0)
        };
        if replace {
            best = candidate;
        }
    }
    Ok(best.clone())
}

/// Avoid rounding an integer before comparison with another exact integer.
fn exact_number(value: &Value) -> Option<f64> {
    match value {
        Value::Float(number) => Some(*number),
        Value::Int(integer) if (*integer as f64) as i128 == i128::from(*integer) => {
            Some(*integer as f64)
        }
        _ => None,
    }
}
