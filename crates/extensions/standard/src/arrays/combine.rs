//! Combining arrays preserves string keys, but merging renumbers integer keys.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value, args,
};

#[derive(Clone, Copy)]
pub(super) enum Collision {
    Merge,
    Replace,
}

#[derive(Clone, Copy)]
pub(super) enum Depth {
    Shallow,
    Recursive,
}

const ARRAY_MERGE_PARAMS: &[Param] = &[Param::new("arrays", ParamType::Array).variadic()];

const ARRAY_REPLACE_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("replacements", ParamType::Array).variadic(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "array_merge",
        ARRAY_MERGE_PARAMS,
        array_merge,
    ));
    registry.function(FunctionEntry::sync(
        "array_replace",
        ARRAY_REPLACE_PARAMS,
        array_replace,
    ));
    registry.function(FunctionEntry::sync(
        "array_merge_recursive",
        ARRAY_MERGE_PARAMS,
        array_merge_recursive,
    ));
    registry.function(FunctionEntry::sync(
        "array_replace_recursive",
        ARRAY_REPLACE_PARAMS,
        array_replace_recursive,
    ));
}

fn array_merge(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Merge, Depth::Shallow)
}

fn array_replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Replace, Depth::Shallow)
}

fn array_merge_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Merge, Depth::Recursive)
}

fn array_replace_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Replace, Depth::Recursive)
}

/// Replacement starts with a copy, retaining even the first array's next index.
fn combine(
    context: &mut dyn Context,
    arguments: &[Value],
    collision: Collision,
    depth: Depth,
) -> NativeResult {
    let mut result = if matches!(collision, Collision::Replace) {
        args::array(arguments, 0)
            .map(|array| (**array).clone())
            .unwrap_or_default()
    } else {
        PhpArray::new()
    };
    for value in arguments
        .iter()
        .skip(usize::from(matches!(collision, Collision::Replace)))
    {
        if let Value::Array(array) = value {
            result = super::combine_recursive::combine_pair(
                context,
                result,
                array.clone(),
                collision,
                depth,
            )?;
        }
    }
    Ok(Value::from(result))
}
