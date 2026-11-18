//! Native functions consume Traversable objects without collecting them first.
//!
//! ```php
//! iterator_count($items);         // advances without calling current() or key()
//! iterator_to_array($items, false); // calls current(), skips key()
//! ```

use crate::{Context, LocalFuture, NativeError, NativeResult, Value};
use phera_value::Object;
use std::{future::Future, pin::Pin, rc::Rc, task::Poll};

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
    /// The VM can advance this cursor directly if its generator is suspended.
    fn generator(&self) -> Option<&Rc<Object>> {
        None
    }

    fn step<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> StepFuture<'a>;
}

/// PHP method calls may wait. A native cursor or a suspended generator can
/// complete a step without allocating a future.
pub enum StepFuture<'a> {
    Ready(std::future::Ready<NativeResult>),
    Pending(LocalFuture<'a, NativeResult>),
}

impl<'a> StepFuture<'a> {
    pub fn ready(result: NativeResult) -> Self {
        Self::Ready(std::future::ready(result))
    }

    /// Native PHP methods use the extension API's asynchronous method ABI.
    pub fn into_local_future(self) -> LocalFuture<'a, NativeResult> {
        match self {
            Self::Ready(ready) => Box::pin(ready),
            Self::Pending(future) => future,
        }
    }
}

impl Future for StepFuture<'_> {
    type Output = NativeResult;

    fn poll(self: Pin<&mut Self>, context: &mut std::task::Context<'_>) -> Poll<NativeResult> {
        match self.get_mut() {
            Self::Ready(ready) => Pin::new(ready).poll(context),
            Self::Pending(future) => future.as_mut().poll(context),
        }
    }
}

pub type TraversalResult = Result<Box<dyn Traversal>, NativeError>;

/// Read the current element without observing an unused key.
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
