//! Native functions consume Traversable objects without collecting them first.
//!
//! ```php
//! iterator_count($items);         // advances without calling current() or key()
//! iterator_to_array($items, false); // calls current(), skips key()
//! ```

use crate::{Context, LocalFuture, NativeError, NativeResult, Value};

#[derive(Clone, Copy)]
pub enum IteratorStep {
    Rewind,
    Advance,
    Valid,
    Current,
    Key,
}

/// A request-local iterator. Values and keys are fetched separately so each language
/// operation calls only the methods PHP calls, in the same order.
pub trait Traversal {
    /// Combined foreach entry from a cursor which can run without the VM.
    fn next_entry(&mut self, _with_key: bool) -> Option<(Value, Option<Value>)> {
        None
    }

    /// Native cursors can complete a step without allocating a future.
    fn step_native(
        &mut self,
        _context: &mut dyn Context,
        _operation: IteratorStep,
    ) -> Option<NativeResult> {
        None
    }

    fn step<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> LocalFuture<'a, NativeResult>;
}

pub type TraversalResult = Result<Box<dyn Traversal>, NativeError>;

/// Poll only iterators whose operations can call PHP.
pub async fn step(
    iterator: &mut dyn Traversal,
    context: &mut dyn Context,
    operation: IteratorStep,
) -> NativeResult {
    match iterator.step_native(context, operation) {
        Some(result) => result,
        None => iterator.step(context, operation).await,
    }
}

/// Open a traversal and read its current element without observing an unused key.
pub async fn current(
    iterator: &mut dyn Traversal,
    context: &mut dyn Context,
    with_key: bool,
) -> Result<(Value, Option<Value>), NativeError> {
    let value = step(iterator, context, IteratorStep::Current).await?;
    let key = if with_key {
        Some(step(iterator, context, IteratorStep::Key).await?)
    } else {
        None
    };
    Ok((value, key))
}
