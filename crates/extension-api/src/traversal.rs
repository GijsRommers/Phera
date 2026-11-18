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
    /// Observe a cursor that can answer without calling PHP or awaiting work.
    fn observation(&self, _operation: IteratorStep) -> Option<NativeResult> {
        None
    }

    fn step<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> LocalFuture<'a, NativeResult>;
}

pub type TraversalResult = Result<Box<dyn Traversal>, NativeError>;

/// Use the synchronous observation when the cursor already has an answer.
pub async fn observe(
    iterator: &mut dyn Traversal,
    context: &mut dyn Context,
    operation: IteratorStep,
) -> NativeResult {
    match iterator.observation(operation) {
        Some(result) => result,
        None => iterator.step(context, operation).await,
    }
}

/// Read the current element without observing an unused key.
pub async fn current(
    iterator: &mut dyn Traversal,
    context: &mut dyn Context,
    with_key: bool,
) -> Result<(Value, Option<Value>), NativeError> {
    let value = observe(iterator, context, IteratorStep::Current).await?;
    let key = if with_key {
        Some(observe(iterator, context, IteratorStep::Key).await?)
    } else {
        None
    };
    Ok((value, key))
}
