//! Sifting keeps the better child above its siblings. Equal priorities keep
//! their place during insertion but extraction moves the last element to the root.
//!
//! ```php
//! $queue->insert('first', 1);
//! $queue->insert('second', 1);
//! echo $queue->extract(); // first
//! ```

use std::{cmp::Ordering, rc::Rc};

use phera_extension_api::{Context, NativeError, Object, Value, value};

use super::{Entry, Heap, Status, read, update};

#[derive(Clone, Copy)]
enum Sift {
    Up(usize),
    ChooseChild(usize),
    Descend { root: usize, child: usize },
    Complete,
}

impl Sift {
    fn pair(&mut self, length: usize) -> Option<(usize, usize)> {
        match *self {
            Self::Up(position) if position != 0 => Some(((position - 1) / 2, position)),
            Self::ChooseChild(root) => {
                let left = root.checked_mul(2)?.checked_add(1)?;
                if left >= length {
                    return None;
                }
                let right = left + 1;
                if right < length {
                    Some((right, left))
                } else {
                    *self = Self::Descend { root, child: left };
                    Some((root, left))
                }
            }
            Self::Descend { root, child } => Some((root, child)),
            _ => None,
        }
    }

    fn ordered(
        self,
        items: &mut [Entry],
        pair: (usize, usize),
        ordering: Ordering,
    ) -> Result<Self, NativeError> {
        let (left, right) = pair;
        let swap = |items: &mut [Entry]| {
            if left >= items.len() || right >= items.len() {
                return Err(NativeError::error("Heap changed during comparison"));
            }
            items.swap(left, right);
            Ok(())
        };
        Ok(match self {
            Self::ChooseChild(root) => Self::Descend {
                root,
                child: if ordering.is_gt() { left } else { right },
            },
            Self::Up(_) if ordering.is_lt() => {
                swap(items)?;
                Self::Up(left)
            }
            Self::Descend { .. } if ordering.is_lt() => {
                swap(items)?;
                Self::ChooseChild(right)
            }
            _ => Self::Complete,
        })
    }
}

pub(super) enum Change {
    Insert(Entry),
    Remove,
}

impl Change {
    fn begin(self, heap: &mut Heap) -> Result<(Sift, Option<Entry>), NativeError> {
        heap.validate(true)?;
        let change = match self {
            Self::Insert(entry) => {
                let position = heap.items.len();
                heap.items.push(entry);
                (Sift::Up(position), None)
            }
            Self::Remove if heap.items.is_empty() => return Ok((Sift::Complete, None)),
            Self::Remove => (Sift::ChooseChild(0), Some(heap.items.swap_remove(0))),
        };
        heap.status = Status::Modifying;
        Ok(change)
    }
}

fn finish(heap: &mut Heap, failed: bool) {
    heap.status = if failed {
        Status::Corrupted
    } else {
        Status::Ready
    };
}

fn native_order(
    context: &mut dyn Context,
    left: &Value,
    right: &Value,
    reverse: bool,
) -> Result<Ordering, NativeError> {
    let (left, right) = if reverse {
        (right, left)
    } else {
        (left, right)
    };
    if let (Value::Int(left), Value::Int(right)) = (left, right) {
        return Ok(left.cmp(right));
    }
    Ok(value::compare(left, right, context.diagnostics())?.cmp(&0))
}

pub(super) fn apply_native(
    context: &mut dyn Context,
    object: &Object,
    change: Change,
) -> Result<Option<Entry>, NativeError> {
    let reverse = object.class.is_instance_of("SplMinHeap");
    update(object, |heap| {
        let (mut sift, removed) = change.begin(heap)?;
        let result = (|| {
            while let Some(pair) = sift.pair(heap.items.len()) {
                let ordering = {
                    let left = heap.items.get(pair.0).ok_or_else(missing)?.weight();
                    let right = heap.items.get(pair.1).ok_or_else(missing)?.weight();
                    native_order(context, left, right, reverse)?
                };
                sift = sift.ordered(&mut heap.items, pair, ordering)?;
            }
            Ok(removed)
        })();
        finish(heap, result.is_err());
        result
    })?
}

pub(super) async fn apply(
    context: &mut dyn Context,
    object: &Rc<Object>,
    change: Change,
) -> Result<Option<Entry>, NativeError> {
    if !context.is_user_method(object, "compare") {
        return apply_native(context, object, change);
    }
    let (mut sift, removed) = update(object, |heap| change.begin(heap))??;
    let result = async {
        while let Some(pair) = sift.pair(read(object, |heap| heap.items.len())?) {
            let values = read(object, |heap| {
                let left = heap.items.get(pair.0).ok_or_else(missing)?.weight();
                let right = heap.items.get(pair.1).ok_or_else(missing)?.weight();
                Ok::<_, NativeError>(vec![left.clone(), right.clone()])
            })??;
            let result = context
                .call_protected_method(object.clone(), "compare", values)
                .await?;
            let order = value::to_int(&result, context.diagnostics())?.cmp(&0);
            sift = update(object, |heap| sift.ordered(&mut heap.items, pair, order))??;
        }
        Ok(removed)
    }
    .await;
    update(object, |heap| finish(heap, result.is_err()))?;
    result
}

fn missing() -> NativeError {
    NativeError::error("Heap changed during comparison")
}

pub(super) fn compare_values(
    context: &mut dyn Context,
    left: &Value,
    right: &Value,
    reverse: bool,
) -> Result<Value, NativeError> {
    Ok(Value::Int(
        native_order(context, left, right, reverse)? as i64
    ))
}
