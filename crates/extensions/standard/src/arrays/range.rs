//! Port of php-src ext/standard/array.c: PHP_FUNCTION(range).
//! Endpoint coercion and step validation retain PHP 8.3 diagnostic order.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::range_endpoint::{ByteMeaning, Endpoint, NumberKind, endpoint, error};

const RANGE_PARAMS: &[Param] = &[
    Param::new("start", ParamType::Mixed),
    Param::new("end", ParamType::Mixed),
    Param::new("step", ParamType::Mixed).optional(),
];

const PREPARED_RANGE_PARAMS: &[Param] = &[
    Param::new("start", ParamType::Mixed),
    Param::new("end", ParamType::Mixed),
    Param::new("step", ParamType::Number).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("range", RANGE_PARAMS, range));
}

/// The endpoint union cannot be declared in ParamType. Parse it before the
/// step so null deprecations and type errors retain PHP's argument order.
fn range(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    for (index, name) in [(0, "start"), (1, "end")] {
        let value = args::value(arguments, index);
        let scalar = matches!(value, Value::Int(_) | Value::Float(_) | Value::String(_));
        let coercible = matches!(value, Value::Null | Value::Bool(_));
        if !scalar && (context.strict_types() || !coercible) {
            let given = crate::arguments::given_type(value);
            return Err(NativeError::type_error(format!(
                "range(): Argument #{} (${name}) must be of type string|int|float, {given} given",
                index + 1
            )));
        }
        if value.is_null() {
            context.deprecated(format!("range(): Passing null to parameter #{} (${name}) of type string|int|float is deprecated",index+1));
        }
    }
    let mut prepared = arguments.to_vec();
    phera_extension_api::prepare_arguments(
        &FunctionEntry::sync("range", PREPARED_RANGE_PARAMS, range_values),
        &mut prepared,
        context,
    )?;
    range_values(context, &prepared)
}

/// A negative step is valid only for a decreasing or constant range.
fn range_values(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let step = Step::parse(arguments)?;
    let mut start = endpoint(context, args::value(arguments, 0), 1, "start")?;
    let mut end = endpoint(context, args::value(arguments, 1), 2, "end")?;
    let characters = reconcile_endpoints(context, &mut start, &mut end, &step);
    let kind = if matches!(start.kind, NumberKind::Floating)
        || matches!(end.kind, NumberKind::Floating)
        || matches!(step.kind, StepKind::Fractional)
    {
        super::range_values::RangeKind::Floating
    } else if characters {
        super::range_values::RangeKind::Character
    } else {
        super::range_values::RangeKind::Integer
    };
    super::range_values::RangeValues::new(start, end, step, kind).collect()
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
