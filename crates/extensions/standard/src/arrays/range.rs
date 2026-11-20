//! `range(1, 5, 2)` is `[1, 3, 5]`: ported from php-src
//! ext/standard/array.c, `PHP_FUNCTION(range)`.
//!
//! The endpoints are read in range_endpoint.rs and the values made in
//! range_values.rs; this file checks the arguments in PHP's order.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::range_endpoint::{ByteMeaning, Endpoint, NumberKind, endpoint, error};
use super::range_values::{RangeKind, RangeValues};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[RANGE];

/// `range(string|int|float $start, string|int|float $end, int|float $step
/// = 1): array`
///
/// Single-byte strings make a range of characters; a float anywhere makes
/// floats. The step's sign does not matter, except that a negative step
/// cannot go up.
///
/// ```php
/// range(1, 5, 2);   // [1, 3, 5]
/// range(5, 1, 2);   // [5, 3, 1]
/// range("a", "e", 2); // ["a", "c", "e"]
/// range(0, 1, 0.25);  // [0.0, 0.25, 0.5, 0.75, 1.0]
/// ```
const RANGE: FunctionEntry = FunctionEntry::sync(
    "range",
    &[
        Param::new("start", ParamType::StringOrNumber),
        Param::new("end", ParamType::StringOrNumber),
        Param::new("step", ParamType::Number).optional(),
    ],
    range,
);

/// A negative step is valid only for a decreasing or constant range.
fn range(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let step = Step::parse(arguments)?;
    let mut start = endpoint(context, args::value(arguments, 0), 1, "start")?;
    let mut end = endpoint(context, args::value(arguments, 1), 2, "end")?;
    let characters = reconcile_endpoints(context, &mut start, &mut end, &step);
    let kind = if matches!(start.kind, NumberKind::Floating)
        || matches!(end.kind, NumberKind::Floating)
        || matches!(step.kind, StepKind::Fractional)
    {
        RangeKind::Floating
    } else if characters {
        RangeKind::Character
    } else {
        RangeKind::Integer
    };
    RangeValues::new(start, end, step, kind).collect()
}

#[derive(Clone, Copy)]
enum StepKind {
    Integral,
    Fractional,
}

pub(super) struct Step {
    pub(super) signed_step: f64,
    pub(super) step: f64,
    kind: StepKind,
    pub(super) integer_step: u64,
}

impl Step {
    /// Validate the step before coercing endpoints, matching PHP's diagnostic order.
    fn parse(arguments: &[Value]) -> Result<Self, NativeError> {
        let signed_step = args::float(arguments, 2).unwrap_or(1.0);
        if !signed_step.is_finite() {
            return Err(error(
                3,
                "step",
                if signed_step.is_nan() {
                    "must be a finite number, NAN provided"
                } else {
                    "must be a finite number, INF provided"
                },
            ));
        }
        if args::int(arguments, 2) == Some(i64::MIN) {
            return Err(error(
                3,
                "step",
                "must be greater than -9223372036854775808",
            ));
        }
        if signed_step == 0.0 {
            return Err(error(3, "step", "cannot be 0"));
        }
        let step = signed_step.abs();
        let fractional = args::int(arguments, 2).is_none()
            && (step.fract() != 0.0 || step >= 9_223_372_036_854_775_808.0);
        let integer_step = args::int(arguments, 2)
            .map(i64::unsigned_abs)
            .unwrap_or(step as u64);
        Ok(Self {
            signed_step,
            step,
            kind: if fractional {
                StepKind::Fractional
            } else {
                StepKind::Integral
            },
            integer_step,
        })
    }
}

/// Mixed character and numeric endpoints warn before numeric range validation.
fn reconcile_endpoints(
    context: &mut dyn Context,
    start: &mut Endpoint,
    end: &mut Endpoint,
    step: &Step,
) -> bool {
    let fractional = matches!(step.kind, StepKind::Fractional);
    let characters = start.byte.is_some() && end.byte.is_some() && !fractional;
    if start.byte.is_some() != end.byte.is_some() {
        let (number, name, other_number, other_name, ambiguous) = if start.byte.is_some() {
            (
                2,
                "end",
                1,
                "start",
                matches!(start.byte_meaning, ByteMeaning::Numeric),
            )
        } else {
            (
                1,
                "start",
                2,
                "end",
                matches!(end.byte_meaning, ByteMeaning::Numeric),
            )
        };
        if !ambiguous {
            context.warning(format!("range(): Argument #{number} (${name}) must be a single byte string if argument #{other_number} (${other_name}) is a single byte string, argument #{other_number} (${other_name}) converted to 0"));
        }
    } else if start.byte.is_some()
        && fractional
        && (!matches!(start.byte_meaning, ByteMeaning::Numeric)
            || !matches!(end.byte_meaning, ByteMeaning::Numeric))
    {
        context.warning("range(): Argument #3 ($step) must be of type int when generating an array of characters, inputs converted to 0".into());
    }
    if characters {
        start.integer = i64::from(start.byte.unwrap_or_default());
        end.integer = i64::from(end.byte.unwrap_or_default());
        start.number = start.integer as f64;
        end.number = end.integer as f64;
    }
    characters
}
