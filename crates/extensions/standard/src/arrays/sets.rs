//! Set operations compare string representations, while key variants retain key identity.

use std::cmp::Ordering;

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value, args,
};

use super::ordering::SortOrder;

#[derive(Clone, Copy)]
enum SetOperation {
    Difference,
    Intersection,
}

#[derive(Clone, Copy)]
enum MatchBy {
    Key,
    Value,
    KeyAndValue,
}

const ARRAY_SET_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("arrays", ParamType::Array).variadic(),
];

const ARRAY_UNIQUE_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("flags", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, body) in [
        ("array_diff", array_diff as phera_extension_api::SyncFn),
        ("array_diff_key", array_diff_key),
        ("array_diff_assoc", array_diff_assoc),
        ("array_intersect", array_intersect),
        ("array_intersect_key", array_intersect_key),
        ("array_intersect_assoc", array_intersect_assoc),
    ] {
        registry.function(FunctionEntry::sync(name, ARRAY_SET_PARAMS, body));
    }
    registry.function(FunctionEntry::sync(
        "array_unique",
        ARRAY_UNIQUE_PARAMS,
        array_unique,
    ));
}

/// PHP builds a string lookup table, converting each value exactly once.
fn array_diff(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if args::array(arguments, 0).is_some_and(|array| array.is_empty()) {
        return Ok(args::value(arguments, 0).clone());
    }
    if let Some(array) = args::array(arguments, 0)
        && array.len() == 1
    {
        if let Some(value) = array.values().next() {
            let search = phera_extension_api::value::to_php_string(value, context.diagnostics())?;
            for other in arguments.iter().skip(1) {
                if let Value::Array(other) = other {
                    for value in other.values() {
                        if search
                            == phera_extension_api::value::to_php_string(
                                value,
                                context.diagnostics(),
                            )?
                        {
                            return Ok(Value::from(PhpArray::new()));
                        }
                    }
                }
            }
        }
        return Ok(args::value(arguments, 0).clone());
    }
    if arguments.len() == 1 {
        return Ok(args::value(arguments, 0).clone());
    }
    let mut excluded = std::collections::HashSet::new();
    for other in arguments.iter().skip(1) {
        if let Value::Array(array) = other {
            for value in array.values() {
                excluded.insert(phera_extension_api::value::to_php_string(
                    value,
                    context.diagnostics(),
                )?);
            }
        }
    }
    if excluded.is_empty() {
        return Ok(args::value(arguments, 0).clone());
    }
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            let string = phera_extension_api::value::to_php_string(value, context.diagnostics())?;
            if !excluded.contains(&string) {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(Value::from(result))
}

fn array_diff_key(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    set(context, arguments, SetOperation::Difference, MatchBy::Key)
}

fn array_diff_assoc(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    set(
        context,
        arguments,
        SetOperation::Difference,
        MatchBy::KeyAndValue,
    )
}

fn array_intersect(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    set(
        context,
        arguments,
        SetOperation::Intersection,
        MatchBy::Value,
    )
}

fn array_intersect_key(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    set(context, arguments, SetOperation::Intersection, MatchBy::Key)
}

fn array_intersect_assoc(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    set(
        context,
        arguments,
        SetOperation::Intersection,
        MatchBy::KeyAndValue,
    )
}

/// Difference rejects a match in any input; intersection needs one in every input.
fn set(
    context: &mut dyn Context,
    arguments: &[Value],
    operation: SetOperation,
    match_by: MatchBy,
) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            let mut keep = true;
            for other in arguments.iter().skip(1) {
                let Value::Array(other) = other else {
                    continue;
                };
                let mut found = false;
                for (other_key, other_value) in other.iter() {
                    if !matches!(match_by, MatchBy::Value) && key != other_key {
                        continue;
                    }
                    if matches!(match_by, MatchBy::Key)
                        || super::ordering::compare_values(
                            context,
                            value,
                            other_value,
                            SortOrder::String { ignore_case: false },
                        )? == Ordering::Equal
                    {
                        found = true;
                        break;
                    }
                }
                if found != matches!(operation, SetOperation::Intersection) {
                    keep = false;
                    break;
                }
            }
            if keep {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(Value::from(result))
}

/// Keep the first occurrence and its key; string comparison is the default.
fn array_unique(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let flags = args::int(arguments, 1).unwrap_or(2);
    let Some(array) = args::array(arguments, 0) else {
        return Ok(Value::from(PhpArray::new()));
    };
    if array.len() <= 1 {
        return Ok(Value::Array(array.clone()));
    }
    if flags == 2 {
        let mut seen = std::collections::HashSet::new();
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            if seen.insert(phera_extension_api::value::to_php_string(
                value,
                context.diagnostics(),
            )?) {
                result.insert(key.clone(), value.clone());
            }
        }
        return Ok(Value::from(result));
    }
    let order = SortOrder::from_flags(flags);
    let entries: Vec<_> = array.iter().collect();
    let indices = super::sort_order::order(entries.len(), |left, right| {
        let (Some((_, left)), Some((_, right))) = (entries.get(left), entries.get(right)) else {
            return Ok(Ordering::Equal);
        };
        super::ordering::compare_values(context, left, right, order)
    })?;
    let mut result = (**array).clone();
    let mut previous: Option<usize> = None;
    for index in indices {
        if let Some(last) = previous
            && let (Some((last_key, last_value)), Some((key, value))) =
                (entries.get(last), entries.get(index))
            && super::ordering::compare_values(context, last_value, value, order)?
                == Ordering::Equal
        {
            if last > index {
                result.remove(last_key);
                previous = Some(index);
            } else {
                result.remove(key);
            }
            continue;
        }
        previous = Some(index);
    }
    Ok(Value::from(result))
}
