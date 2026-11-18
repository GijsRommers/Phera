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
    fn step<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> LocalFuture<'a, NativeResult>;
}

pub type TraversalResult = Result<Box<dyn Traversal>, NativeError>;

/// Open a traversal and read its current element without observing an unused key.
pub async fn current(
    iterator: &mut dyn Traversal,
    context: &mut dyn Context,
    with_key: bool,
) -> Result<(Value, Option<Value>), NativeError> {
    let value = iterator.step(context, IteratorStep::Current).await?;
    let key = if with_key {
        Some(iterator.step(context, IteratorStep::Key).await?)
    } else {
        None
    };
    Ok((value, key))
}
