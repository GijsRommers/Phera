//! Stable array sorting with PHP-compatible comparison traversal.
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};
use std::cmp::Ordering;
const SORT: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("flags", ParamType::Int).optional(),
];
const NATURAL: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];
pub(crate) fn register(registry: &mut Registry) {
    for (name, body) in [
        ("sort", sort as phera_extension_api::SyncFn),
        ("rsort", rsort),
        ("asort", asort),
        ("arsort", arsort),
        ("ksort", ksort),
        ("krsort", krsort),
    ] {
        registry.function(FunctionEntry::sync(name, SORT, body));
    }
    registry.function(FunctionEntry::sync("natsort", NATURAL, natsort));
    registry.function(FunctionEntry::sync("natcasesort", NATURAL, natcasesort));
}
fn sort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, false, false, false, None)
}
fn rsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, false, false, true, None)
}
fn asort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, false, false, None)
}
fn arsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, false, true, None)
}
fn ksort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, true, false, None)
}
fn krsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, true, true, None)
}
fn natsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, false, false, Some(6))
}
fn natcasesort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true, false, false, Some(14))
}
/// Original ordinals break ties even for descending sorts, retaining PHP 8 stability.
fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    preserve: bool,
    keys: bool,
    reverse: bool,
    fixed: Option<i64>,
) -> NativeResult {
    let flags = fixed.unwrap_or_else(|| args::int(arguments, 1).unwrap_or_default());
    super::stack::mutate(arguments, |array| {
        let entries: Vec<(ArrayKey, Value)> = array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let indices = super::sort_order::order(entries.len(), |left, right| {
            let (Some(a), Some(b)) = (entries.get(left), entries.get(right)) else {
                return Ok(Ordering::Equal);
            };
            let order = if keys {
                super::ordering::compare_values(
                    context,
                    &super::key_value(&a.0),
                    &super::key_value(&b.0),
                    flags,
                )?
            } else {
                super::ordering::compare_values(context, &a.1, &b.1, flags)?
            };
            let order = if reverse { order.reverse() } else { order };
            Ok(order.then_with(|| left.cmp(&right)))
        })?;
        let mut result = PhpArray::new();
        for index in indices {
            if let Some((key, value)) = entries.get(index) {
                if preserve {
                    result.insert(key.clone(), value.clone());
                } else {
                    result.push(value.clone())?;
                }
            }
        }
        result.reset();
        *array = result;
        Ok(Value::Bool(true))
    })
}
