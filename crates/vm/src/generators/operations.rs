//! Public operations distinguish observation, first-yield initialization and resumption.
//!
//! ```php
//! $generator->current(); // runs to the first yield
//! $generator->send($value); // delivers the value, then runs to the next yield
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{GeneratorOperation, NativeError, NativeResult, traversal::IteratorStep};
use phera_value::{Object, Value};

use crate::{Vm, exceptions::Thrown};

use super::resume::{Motion, is_open, needs_priming};
use super::{Generator, Item, State, delegation::Delegate, execution::Input, state};

/// Reading the current item of an already-started local generator never executes PHP.
pub(crate) fn local_observation(
    object: &Object,
    operation: &GeneratorOperation,
) -> Option<NativeResult> {
    let (step, by_reference) = match *operation {
        GeneratorOperation::Method(step) => (step, false),
        GeneratorOperation::Cursor { step, by_reference } => (step, by_reference),
        _ => return None,
    };
    if !matches!(
        step,
        IteratorStep::Current | IteratorStep::Key | IteratorStep::Valid
    ) {
        return None;
    }
    match state(object, |generator| {
        if needs_priming(generator)
            || matches!(
                generator.state,
                State::Delegating(_, _, Delegate::Generator(_))
            )
        {
            return None;
        }
        Some(observe(generator, step, by_reference))
    }) {
        Ok(observed) => observed.map(Ok),
        Err(error) => Some(Err(error)),
    }
}

fn observe(generator: &mut Generator, step: IteratorStep, by_reference: bool) -> Value {
    match step {
        IteratorStep::Current => current_value(generator, by_reference),
        IteratorStep::Key => active_item(generator)
            .map(|item| item.key.clone())
            .unwrap_or_default(),
        IteratorStep::Valid => Value::Bool(is_open(generator)),
        _ => Value::Null,
    }
}

impl Vm {
    pub(crate) async fn operate_generator(
        &mut self,
        object: Rc<Object>,
        operation: GeneratorOperation,
    ) -> NativeResult {
        let (step, by_reference) = match operation {
            GeneratorOperation::Method(step) => (Some(step), false),
            GeneratorOperation::Cursor { step, by_reference } => (Some(step), by_reference),
            _ => (None, false),
        };
        if state(&object, needs_priming)? {
            let result = self.drive_generator(object.clone(), Motion::Observe).await;
            state(&object, |generator| generator.first_yield = true)?;
            result?;
        }
        match operation {
            GeneratorOperation::GetReturn => state(&object, |generator| match &generator.state {
                State::Returned(value) => Ok(value.clone()),
                _ => Err(NativeError::throw(
                    "Exception",
                    "Cannot get return value of a generator that hasn't returned",
                )),
            })?,
            GeneratorOperation::Send(value) => self
                .advance_generator(object, Input::Send(value))
                .await
                .map(|leaf| current(&leaf, false))?,
            GeneratorOperation::Throw(value) => {
                let Value::Object(exception) = value else {
                    return Err(internal_error!("generator Throwable argument"));
                };
                let error = Thrown {
                    object: exception,
                    skip_handlers: None,
                }
                .into_error();
                self.advance_generator(object, Input::Throw(error))
                    .await
                    .map(|leaf| current(&leaf, false))?
            }
            _ => match step {
                Some(IteratorStep::Advance) => {
                    self.advance_generator(object, Input::Send(Value::Null))
                        .await?;
                    Ok(Value::Null)
                }
                Some(IteratorStep::Rewind) => state(&object, |generator| {
                    if generator.first_yield {
                        Ok(Value::Null)
                    } else {
                        Err(NativeError::throw(
                            "Exception",
                            "Cannot rewind a generator that was already run",
                        ))
                    }
                })?,
                Some(step) => {
                    let leaf = self.drive_generator(object, Motion::Observe).await?;
                    state(&leaf, |generator| observe(generator, step, by_reference))
                }
                None => Err(internal_error!("generator operation")),
            },
        }
    }
}

fn active_item(generator: &mut Generator) -> Option<&mut Item> {
    if is_open(generator) {
        generator.item.as_mut()
    } else {
        None
    }
}

fn current(object: &Object, by_reference: bool) -> NativeResult {
    state(object, |generator| current_value(generator, by_reference))
}

fn current_value(generator: &mut Generator, by_reference: bool) -> Value {
    let Some(item) = active_item(generator) else {
        return Value::Null;
    };
    if !by_reference {
        return item.value.deref().clone();
    }
    if !item.value.is_reference() {
        item.value = Value::Reference(Rc::new(RefCell::new(std::mem::take(&mut item.value))));
    }
    item.value.clone()
}
