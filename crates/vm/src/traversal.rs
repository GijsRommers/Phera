//! Shared cursor acquisition for foreach, iterator functions and unpacking.
//!
//! ```php
//! foreach ($aggregate as $key => $value) { /* getIterator() first */ }
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, NativeError, NativeIterator, NativeResult,
    traversal::{IteratorStep, StepFuture, Traversal, TraversalResult},
};
use phera_value::{Object, Value};

use crate::{Vm, classes::operations::ClassOperations};

enum Cursor {
    Php(Rc<Object>),
    Native(Box<dyn NativeIterator>),
}

impl Traversal for Cursor {
    fn step<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> StepFuture<'a> {
        match self {
            Self::Php(object) => StepFuture::Pending(context.call_protocol_method(
                object.clone(),
                match operation {
                    IteratorStep::Rewind => "rewind",
                    IteratorStep::Advance => "next",
                    IteratorStep::Valid => "valid",
                    IteratorStep::Current => "current",
                    IteratorStep::Key => "key",
                },
                Vec::new(),
            )),
            Self::Native(iterator) => {
                StepFuture::ready(native_step(iterator.as_mut(), context, operation))
            }
        }
    }
}

pub(crate) fn native_step(
    iterator: &mut dyn NativeIterator,
    context: &mut dyn Context,
    operation: IteratorStep,
) -> NativeResult {
    match operation {
        IteratorStep::Rewind => iterator.rewind(context).map(|()| Value::Null),
        IteratorStep::Advance => iterator.advance(context).map(|()| Value::Null),
        IteratorStep::Valid => iterator.valid(context).map(Value::Bool),
        IteratorStep::Current => iterator.current(context),
        IteratorStep::Key => iterator.key(context),
    }
}

pub(crate) fn reference_error() -> NativeError {
    NativeError::error("An iterator cannot be used with foreach by reference")
}

impl Vm {
    /// Aggregate chains resolve without growing a Rust stack.
    pub(crate) async fn make_traversal(
        &mut self,
        mut object: Rc<Object>,
        by_reference: bool,
    ) -> TraversalResult {
        for _ in 0..self.frame_limit {
            let class = self
                .classes
                .for_object(&object)
                .ok_or_else(|| internal_error!("traversal class"))?;
            if crate::generators::generator_object(&object) {
                return Ok(Box::new(crate::generators::class::Cursor::open(
                    object,
                    by_reference,
                )?));
            }
            let factory = class.hooks.as_ref().and_then(|hooks| hooks.iterator);
            if let Some(factory) = factory {
                if by_reference {
                    return Err(reference_error());
                }
                let cursor = factory(self, object)?;
                return Ok(Box::new(Cursor::Native(cursor)));
            }
            if class.operations.has(ClassOperations::ITERATOR) {
                if by_reference {
                    return Err(reference_error());
                }
                return Ok(Box::new(Cursor::Php(object)));
            }
            let invalid = || {
                NativeError::throw(
                    "Exception",
                    format!(
                        "Objects returned by {}::getIterator() must be traversable or implement interface Iterator",
                        class.name
                    ),
                )
            };
            if !class.operations.has(ClassOperations::AGGREGATE) {
                return Err(invalid());
            }
            let value = self
                .call_protocol_method(object.clone(), "getIterator", Vec::new())
                .await?;
            let Value::Object(iterator) = value.deref().clone() else {
                return Err(invalid());
            };
            if !iterator.class.is_traversable() || Rc::ptr_eq(&iterator, &object) {
                return Err(invalid());
            }
            object = iterator;
        }
        Err(NativeError::error(
            "IteratorAggregate nesting limit exceeded",
        ))
    }
}
