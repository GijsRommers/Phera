//! Explicit recursive merge frames preserve PHP collisions without Rust stack recursion.

use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

use super::combine::{Collision, Depth};

struct Frame {
    result: PhpArray,
    source: Rc<PhpArray>,
    position: Option<usize>,
    parent_key: Option<ArrayKey>,
    identities: Vec<*const PhpArray>,
}

impl Frame {
    fn new(
        result: PhpArray,
        source: Rc<PhpArray>,
        parent_key: Option<ArrayKey>,
        identities: Vec<*const PhpArray>,
    ) -> Self {
        Self {
            result,
            position: source.first_position(),
            source,
            parent_key,
            identities,
        }
    }

    /// Advance before descending so a completed child resumes at the next entry.
    fn next_entry(&mut self) -> Option<(ArrayKey, Value)> {
        let position = self.position?;
        let (key, value) = self.source.entry_at(position)?;
        let entry = (key.clone(), value.clone());
        self.position = self.source.next_position(position);
        Some(entry)
    }
}

enum Resolution {
    Store(Value),
    Descend(Frame),
}

/// Explicit frames reproduce recursive collisions without using the Rust stack.
pub(super) fn combine_pair(
    context: &mut dyn Context,
    result: PhpArray,
    source: Rc<PhpArray>,
    collision: Collision,
    depth: Depth,
) -> Result<PhpArray, NativeError> {
    let mut frames = vec![Frame::new(result, source, None, Vec::new())];
    loop {
        let Some(frame) = frames.last_mut() else {
            return Ok(PhpArray::new());
        };
        let Some((key, value)) = frame.next_entry() else {
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
        // Merge treats integer keys as positions; replace treats them as names.
        if matches!(collision, Collision::Merge) && matches!(key, ArrayKey::Int(_)) {
            frame.result.push(value)?;
            continue;
        }
        let previous = frame.result.get(&key).cloned();
        let resolution = match (depth, previous) {
            (Depth::Recursive, Some(previous)) => {
                resolve(context, key.clone(), previous, value, collision)?
            }
            _ => Resolution::Store(value),
        };
        match resolution {
            Resolution::Store(value) => {
                if let Some(frame) = frames.last_mut() {
                    frame.result.insert(key, value);
                }
            }
            Resolution::Descend(child) => {
                check_recursion(&frames, &child.identities)?;
                frames.push(child);
            }
        }
    }
}

/// Recursive replacement descends only into two arrays; merging also combines scalars.
fn resolve(
    context: &mut dyn Context,
    key: ArrayKey,
    previous: Value,
    value: Value,
    collision: Collision,
) -> Result<Resolution, NativeError> {
    let left = previous.deref().clone();
    let right = value.deref().clone();
    let right = if right.is_object() && matches!(collision, Collision::Merge) {
        Value::from(phera_extension_api::value::to_array(
            &right,
            context.diagnostics(),
        ))
    } else {
        right
    };
    if matches!(collision, Collision::Replace) && !(left.is_array() && right.is_array()) {
        return Ok(Resolution::Store(value));
    }
    if let (Value::Reference(left_reference), Value::Reference(right_reference)) =
        (&previous, &value)
        && Rc::ptr_eq(left_reference, right_reference)
    {
        return Err(NativeError::error("Recursion detected"));
    }
    let mut destination = destination(context, &left)?;
    if let Value::Array(source) = right {
        let mut identities = Vec::new();
        if let Value::Array(array) = &left {
            identities.push(Rc::as_ptr(array));
        }
        if matches!(collision, Collision::Replace) {
            identities.push(Rc::as_ptr(&source));
        }
        return Ok(Resolution::Descend(Frame::new(
            destination,
            source,
            Some(key),
            identities,
        )));
    }
    destination.push(right)?;
    Ok(Resolution::Store(Value::from(destination)))
}

/// Null collisions become a one-element array rather than PHP's empty null cast.
fn destination(context: &mut dyn Context, value: &Value) -> Result<PhpArray, NativeError> {
    Ok(match value {
        Value::Array(array) => (**array).clone(),
        Value::Null => {
            let mut array = PhpArray::new();
            array.push(Value::Null)?;
            array
        }
        _ => phera_extension_api::value::to_array(value, context.diagnostics()),
    })
}

/// Preserve PHP's recursion error while enforcing the runtime's explicit depth limit.
fn check_recursion(frames: &[Frame], identities: &[*const PhpArray]) -> Result<(), NativeError> {
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
    Ok(())
}
