//! A driver walks generator delegates on the heap and routes sends and exceptions.
//!
//! ```php
//! function outer($inner) { return yield from $inner; }
//! $outer = outer($inner);
//! $outer->send('reply'); // reaches the innermost suspended yield
//! ```

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::{Vm, calls::execution_loop::YIELD_AFTER_STEPS};

use super::{Generator, Item, State, state};
use super::{activation::Activation, delegation::Delegate, execution::Input};

pub(super) enum Motion {
    Observe,
    Advance(Input),
}

enum Position {
    Local,
    Delegates(Rc<Object>),
    Returned(Value),
    Failed,
}

struct Drive {
    ancestors: Vec<Rc<Object>>,
    seen: HashSet<*const Object>,
}

impl Vm {
    pub(super) async fn advance_generator(
        &mut self,
        object: Rc<Object>,
        input: Input,
    ) -> Result<Rc<Object>, NativeError> {
        if !state(&object, |generator| is_open(generator))? {
            if let Input::Throw(error) = input {
                return Err(error);
            }
            return Ok(object);
        }
        state(&object, |generator| generator.first_yield = false)?;
        self.drive_generator(object, Motion::Advance(input)).await
    }

    pub(super) async fn drive_generator(
        &mut self,
        mut object: Rc<Object>,
        mut motion: Motion,
    ) -> Result<Rc<Object>, NativeError> {
        let mut drive = Drive {
            ancestors: Vec::new(),
            seen: HashSet::new(),
        };
        loop {
            let position = state(&object, |generator| match &generator.state {
                State::Delegating(_, _, Delegate::Generator(delegate)) => {
                    Position::Delegates(delegate.clone())
                }
                State::Returned(value) => Position::Returned(value.clone()),
                State::Failed => Position::Failed,
                _ => Position::Local,
            })?;
            let (activation, input) = match position {
                Position::Delegates(delegate) => {
                    drive.seen.insert(Rc::as_ptr(&object));
                    if drive.ancestors.len() >= self.frame_limit {
                        let error = NativeError::error(format!(
                            "Maximum call stack depth of {} exceeded",
                            self.frame_limit,
                        ));
                        leave_delegate(&object, Input::Throw(error))?
                    } else if drive.seen.contains(&Rc::as_ptr(&delegate)) {
                        let input = Input::Throw(impossible_delegation());
                        leave_delegate(&object, input)?
                    } else {
                        drive.ancestors.push(object);
                        if drive.ancestors.len().is_multiple_of(YIELD_AFTER_STEPS) {
                            tokio::task::yield_now().await;
                        }
                        object = delegate;
                        continue;
                    }
                }
                Position::Returned(value) => {
                    let Some(parent) = drive.ancestors.pop() else {
                        return Ok(object);
                    };
                    drive.seen.remove(&Rc::as_ptr(&parent));
                    if matches!(motion, Motion::Observe) {
                        retain_delegate_value(&parent, &object, value)?;
                        return Ok(parent);
                    }
                    object = parent;
                    leave_delegate(&object, Input::Complete(value))?
                }
                Position::Failed => {
                    let Some(parent) = drive.ancestors.pop() else {
                        return Ok(object);
                    };
                    drive.seen.remove(&Rc::as_ptr(&parent));
                    object = parent;
                    leave_delegate(
                        &object,
                        Input::Throw(NativeError::throw(
                            "ClosedGeneratorException",
                            "Generator yielded from aborted, no return value available",
                        )),
                    )?
                }
                Position::Local => {
                    if matches!(&motion, Motion::Observe) && !state(&object, needs_priming)? {
                        return Ok(object);
                    }
                    let input = match std::mem::replace(&mut motion, Motion::Observe) {
                        Motion::Observe => Input::Begin,
                        Motion::Advance(input) => input,
                    };
                    match claim(&object, input)? {
                        Some(claimed) => claimed,
                        None => return Ok(object),
                    }
                }
            };

            let mut outcome = self
                .run_generator_frame(&object, activation, input, &mut drive.ancestors)
                .await;
            loop {
                match outcome {
                    Ok(()) => break,
                    Err(error) => {
                        let Some(parent) = drive.ancestors.pop() else {
                            return Err(error);
                        };
                        drive.seen.remove(&Rc::as_ptr(&parent));
                        object = parent;
                        let (activation, input) = leave_delegate(&object, Input::Throw(error))?;
                        outcome = self
                            .run_generator_frame(&object, activation, input, &mut drive.ancestors)
                            .await;
                    }
                }
            }
            let finished = !state(&object, |generator| is_open(generator))?;
            if finished && !drive.ancestors.is_empty() {
                // A child that just returned continues the yielding expression in its parent.
                motion = Motion::Advance(Input::Begin);
            } else {
                motion = Motion::Observe;
            }
        }
    }
}

fn claim(object: &Object, input: Input) -> Result<Option<(Activation, Input)>, NativeError> {
    state(object, |generator| {
        let previous = std::mem::replace(&mut generator.state, State::Running);
        match previous {
            State::Created(activation) => Ok(Some((activation, Input::Begin))),
            State::Ready(activation) => Ok(Some((activation, input))),
            State::Yielded(activation, destination) => {
                Ok(Some((activation, input.at_yield(destination))))
            }
            State::Delegating(activation, destination, delegate) => {
                let input = match input {
                    Input::Throw(_) => input,
                    Input::Complete(value) => Input::Receive(destination, value),
                    _ => Input::Poll(destination, delegate),
                };
                Ok(Some((activation, input)))
            }
            other => {
                let running = matches!(other, State::Running);
                generator.state = other;
                if running {
                    Err(NativeError::error(
                        "Cannot resume an already running generator",
                    ))
                } else {
                    Ok(None)
                }
            }
        }
    })?
}

fn leave_delegate(object: &Object, input: Input) -> Result<(Activation, Input), NativeError> {
    claim(object, input)?.ok_or_else(|| internal_error!("return to a generator delegate"))
}

fn retain_delegate_value(parent: &Object, child: &Object, value: Value) -> Result<(), NativeError> {
    let last = state(child, |child| {
        child.item.as_ref().map(|item| item.value.clone())
    })?;
    state(parent, |parent| {
        let previous = std::mem::replace(&mut parent.state, State::Failed);
        let State::Delegating(mut activation, destination, _) = previous else {
            parent.state = previous;
            return Err(internal_error!("closed generator delegate"));
        };
        activation.set_result(destination, value)?;
        parent.state = State::Ready(activation);
        if let Some(value) = last {
            let key = parent.item.take().map(|item| item.key).unwrap_or_default();
            parent.item = Some(Item { key, value });
        }
        Ok(())
    })?
}

pub(super) fn is_open(generator: &Generator) -> bool {
    !matches!(generator.state, State::Returned(_) | State::Failed)
}

pub(super) fn needs_priming(generator: &mut Generator) -> bool {
    matches!(generator.state, State::Created(_) | State::Running) && generator.item.is_none()
}

pub(super) fn impossible_delegation() -> NativeError {
    NativeError::error("Impossible to yield from the Generator being currently run")
}
