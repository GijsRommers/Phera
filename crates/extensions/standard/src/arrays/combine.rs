//! Combining arrays preserves string keys, but merging renumbers integer keys.
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};
use std::rc::Rc;

const MERGE: &[Param] = &[Param::new("arrays", ParamType::Array).variadic()];
const REPLACE: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("replacements", ParamType::Array).variadic(),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("array_merge", MERGE, merge));
    registry.function(FunctionEntry::sync("array_replace", REPLACE, replace));
    registry.function(FunctionEntry::sync(
        "array_merge_recursive",
        MERGE,
        merge_recursive,
    ));
    registry.function(FunctionEntry::sync(
        "array_replace_recursive",
        REPLACE,
        replace_recursive,
    ));
}
fn merge(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, false, false)
}
fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, true, false)
}
fn merge_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, false, true)
}
fn replace_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, true, true)
}

/// Replacement starts with a copy, retaining even the first array's next index.
fn combine(
    context: &mut dyn Context,
    arguments: &[Value],
    replace: bool,
    recursive: bool,
) -> NativeResult {
    let mut result = if replace {
        args::array(arguments, 0)
            .map(|array| (**array).clone())
            .unwrap_or_default()
    } else {
        PhpArray::new()
    };
    for value in arguments.iter().skip(usize::from(replace)) {
        if let Value::Array(array) = value {
            result = combine_pair(context, result, array.clone(), replace, recursive)?;
        }
    }
    Ok(Value::from(result))
}
struct Frame {
    result: PhpArray,
    source: Rc<PhpArray>,
    position: Option<usize>,
    parent_key: Option<ArrayKey>,
    identities: Vec<*const PhpArray>,
}
/// Explicit frames reproduce recursive collisions without using the Rust stack.
fn combine_pair(
    context: &mut dyn Context,
    result: PhpArray,
    source: Rc<PhpArray>,
    replace: bool,
    recursive: bool,
) -> Result<PhpArray, NativeError> {
    let mut frames = vec![Frame {
        result,
        position: source.first_position(),
        source,
        parent_key: None,
        identities: Vec::new(),
    }];
    loop {
        let Some(frame) = frames.last_mut() else {
            return Ok(PhpArray::new());
        };
        let entry = frame.position.and_then(|position| {
            frame
                .source
                .entry_at(position)
                .map(|(key, value)| (key.clone(), value.clone()))
        });
        let Some((key, value)) = entry else {
            let Some(completed) = frames.pop() else {
                return Ok(PhpArray::new());
            };
            if let Some(parent) = frames.last_mut() {
                if let Some(key) = completed.parent_key {
                    parent.result.insert(key, Value::from(completed.result));
                }
                continue;
            }
            return Ok(completed.result);
        };
        frame.position = frame
            .position
            .and_then(|position| frame.source.next_position(position));
        if !replace && matches!(key, ArrayKey::Int(_)) {
            frame.result.push(value)?;
            continue;
        }
        let previous = frame.result.get(&key).cloned();
        if recursive && let Some(previous) = previous {
            let left = previous.deref().clone();
            let right = value.deref().clone();
            let right = if right.is_object() && !replace {
                Value::from(phera_extension_api::value::to_array(
                    &right,
                    context.diagnostics(),
                ))
            } else {
                right
            };
            if !replace || (left.is_array() && right.is_array()) {
                if let (Value::Reference(a), Value::Reference(b)) = (&previous, &value)
                    && Rc::ptr_eq(a, b)
                {
                    return Err(NativeError::error("Recursion detected"));
                }
                let mut destination = match &left {
                    Value::Array(array) => (**array).clone(),
                    Value::Null => {
                        let mut array = PhpArray::new();
                        array.push(Value::Null)?;
                        array
                    }
                    _ => phera_extension_api::value::to_array(&left, context.diagnostics()),
                };
                if let Value::Array(source) = right {
                    let mut identities = Vec::new();
                    if let Value::Array(array) = &left {
                        identities.push(Rc::as_ptr(array));
                    }
                    if replace {
                        identities.push(Rc::as_ptr(&source));
                    }
                    if frames.len() > 10_000 {
                        return Err(NativeError::error("Maximum array recursion depth exceeded"));
                    }
                    if frames.iter().any(|frame| {
                        identities
                            .iter()
                            .any(|identity| frame.identities.contains(identity))
                    }) {
                        return Err(NativeError::error("Recursion detected"));
                    }
                    frames.push(Frame {
                        result: destination,
                        position: source.first_position(),
                        source,
                        parent_key: Some(key),
                        identities,
                    });
                    continue;
                }
                destination.push(right)?;
                if let Some(frame) = frames.last_mut() {
                    frame.result.insert(key, Value::from(destination));
                }
                continue;
            }
        }
        if let Some(frame) = frames.last_mut() {
            frame.result.insert(key, value);
        }
    }
}
