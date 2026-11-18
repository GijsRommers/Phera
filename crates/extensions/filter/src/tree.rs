//! Walk nested arrays without using the Rust stack for PHP nesting.
//!
//! ```php
//! filter_var_array(['x' => ['1', '2']], FILTER_VALIDATE_INT);
//! // ['x' => [1, 2]]
//! ```

use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value};

use crate::apply::{self, Leaf};
use crate::{exceptions, options::Options};

struct Frame {
    source: Rc<PhpArray>,
    output: PhpArray,
    position: Option<usize>,
    parent: Option<(ArrayKey, Value)>,
}

impl Frame {
    fn new(source: Rc<PhpArray>, parent: Option<(ArrayKey, Value)>) -> Self {
        Self {
            output: (*source).clone(),
            position: source.first_position(),
            source,
            parent,
        }
    }

    fn next(&mut self) -> Option<(ArrayKey, Value)> {
        let position = self.position?;
        self.position = self.source.next_position(position);
        let (key, value) = self.source.entry_at(position)?;
        Some((key, value.clone()))
    }

    fn store(&mut self, key: ArrayKey, original: Value, result: Value) {
        if let Value::Reference(reference) = original {
            // PHP filters referenced array elements in place.
            *reference.borrow_mut() = result;
        } else {
            self.output.insert(key, result);
        }
    }
}

pub async fn apply(
    context: &mut dyn Context,
    array: Rc<PhpArray>,
    options: &Options,
) -> NativeResult {
    let mut active = HashSet::from([Rc::as_ptr(&array)]);
    let mut frames = vec![Frame::new(array, None)];
    let mut exception = None;
    while let Some(frame) = frames.last_mut() {
        if let Some((key, original)) = frame.next() {
            let item = original.deref().clone();
            if let Value::Array(nested) = item {
                if active.insert(Rc::as_ptr(&nested)) {
                    frames.push(Frame::new(nested, Some((key, original))));
                }
            } else {
                match apply::leaf(context, item, options, exception.is_some()).await? {
                    Leaf::Value(filtered) => frame.store(key, original, filtered),
                    Leaf::Threw { replacement, error } => {
                        frame.store(key, original, replacement);
                        exception = Some(match exception {
                            Some(previous) => exceptions::chain(context, error, previous),
                            None => error,
                        });
                    }
                }
            }
        } else if let Some(done) = frames.pop() {
            active.remove(&Rc::as_ptr(&done.source));
            let result = Value::from(done.output);
            if let Some((key, original)) = done.parent
                && let Some(parent) = frames.last_mut()
            {
                parent.store(key, original, result);
            } else {
                return exception.map_or(Ok(result), Err);
            }
        }
    }
    unreachable!("the root frame returns its result")
}
