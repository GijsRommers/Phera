//! Interpret one installed generator frame until it yields, delegates or returns.
//!
//! ```php
//! function values() {
//!     try { yield 1; } catch (Exception $e) { yield $e->getMessage(); }
//! }
//! ```
//!
//! Injected throws enter the same handler table as exceptions raised by PHP instructions.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::rc::Rc;

use crate::{Vm, calls::execution_loop::BatchExit};

use super::{
    Item, State, Suspension, activation::Activation, delegation::Delegate, resume::Ancestors, state,
};

pub(super) enum Input {
    Begin,
    Send(Value),
    Receive(Reg, Value),
    Complete(Value),
    Throw(NativeError),
    Poll(Reg, Delegate),
}

impl Input {
    pub(super) fn at_yield(self, destination: Reg) -> Self {
        match self {
            Self::Send(value) | Self::Complete(value) => Self::Receive(destination, value),
            other => other,
        }
    }
}

pub(super) enum Progress {
    Visible,
    Delegated,
    Complete,
}

enum Pause {
    Value(Reg, Option<Value>, Value),
    Delegating(Reg, Delegate, Option<Item>),
    Return(Value),
}

impl Vm {
    pub(super) async fn run_generator_frame(
        &mut self,
        object: &Rc<Object>,
        activation: Activation,
        input: Input,
        ancestors: &mut Ancestors,
    ) -> Result<Progress, NativeError> {
        let delegated = !ancestors.is_empty();
        if let Some(site) = self.generator_traces.last_mut() {
            site.active = Some(Rc::as_ptr(object));
            if delegated {
                std::mem::swap(&mut site.ancestors, ancestors);
            }
        }
        let result = self.run_traced_generator(object, activation, input).await;
        if let Some(site) = self.generator_traces.last_mut() {
            site.active = None;
            if delegated {
                std::mem::swap(&mut site.ancestors, ancestors);
            }
        }
        result
    }

    async fn run_traced_generator(
        &mut self,
        object: &Rc<Object>,
        activation: Activation,
        input: Input,
    ) -> Result<Progress, NativeError> {
        let stop = self.call_stack.depth();
        // The caller has already claimed the frame. Return it on every suspension.
        let caller_silence = self.diagnostic_reporter.silence();
        let storage = self.restore_generator_frame(activation);
        let outcome = self.run_generator_instructions(object, input, stop).await;
        let generator_silence = self.diagnostic_reporter.suspend_silence(caller_silence);
        state(object, |generator| match outcome {
            Ok(Pause::Value(destination, key, value)) => {
                generator.publish(key, value);
                let mut activation = self.suspend_generator_frame(storage)?;
                activation.silence = generator_silence;
                generator.state = State::Yielded(activation, destination);
                Ok(Progress::Visible)
            }
            Ok(Pause::Delegating(destination, delegate, item)) => {
                let progress = if item.is_some() {
                    generator.item = item;
                    Progress::Visible
                } else {
                    Progress::Delegated
                };
                let mut activation = self.suspend_generator_frame(storage)?;
                activation.silence = generator_silence;
                generator.state = State::Delegating(activation, destination, delegate);
                Ok(progress)
            }
            Ok(Pause::Return(value)) => {
                generator.state = State::Returned(value);
                Ok(Progress::Complete)
            }
            Err(error) => {
                generator.state = State::Failed;
                Err(error)
            }
        })?
    }

    async fn run_generator_instructions(
        &mut self,
        object: &Rc<Object>,
        mut input: Input,
        stop: usize,
    ) -> Result<Pause, NativeError> {
        loop {
            match input {
                Input::Throw(error) => self.handle_exception(error, stop)?,
                Input::Receive(destination, value) => self.set(destination, value),
                Input::Poll(destination, mut delegate) => {
                    match self.poll_generator_delegate(&mut delegate).await {
                        Ok(Some(item)) => {
                            return Ok(Pause::Delegating(destination, delegate, Some(item)));
                        }
                        Ok(None) => self.set(destination, Value::Null),
                        Err(error) => self.handle_exception(error, stop)?,
                    }
                }
                Input::Begin | Input::Send(_) | Input::Complete(_) => {}
            }
            let result = if self.debugger.is_some() || self.soundness_oracle.is_some() {
                self.interpret(stop).await?
            } else {
                let mut work = 0;
                match self.run_batch(stop, &mut work) {
                    Ok(BatchExit::GeneratorSuspended)
                        if !self.pending_callbacks && !self.generator_releases_pending() =>
                    {
                        Value::Null
                    }
                    other => self.continue_execution(stop, work, Some(other)).await?,
                }
            };
            if self.call_stack.depth() == stop {
                return Ok(Pause::Return(result));
            }
            let event = self
                .generator_suspension
                .take()
                .ok_or_else(|| internal_error!("generator suspension"))?;
            match event {
                Suspension::Yield {
                    destination,
                    key,
                    value,
                } => {
                    return Ok(Pause::Value(destination, key, value));
                }
                Suspension::Delegate {
                    destination,
                    source,
                } => match self.generator_delegate_source(source).await {
                    Ok(Delegate::Generator(delegate)) => {
                        input = match validate_new_delegate(object, &delegate) {
                            Ok(()) => {
                                return Ok(Pause::Delegating(
                                    destination,
                                    Delegate::Generator(delegate),
                                    None,
                                ));
                            }
                            Err(error) => Input::Throw(error),
                        };
                    }
                    Ok(delegate) => input = Input::Poll(destination, delegate),
                    Err(error) => input = Input::Throw(error),
                },
            }
        }
    }
}

fn validate_new_delegate(object: &Rc<Object>, delegate: &Rc<Object>) -> Result<(), NativeError> {
    if Rc::ptr_eq(object, delegate) {
        return Err(super::resume::impossible_delegation());
    }
    state(delegate, |delegate| match delegate.state {
        State::Running => Err(super::resume::impossible_delegation()),
        State::Returned(_) | State::Failed => Err(NativeError::error(
            "Generator passed to yield from was aborted without proper return and is unable to continue",
        )),
        _ => Ok(()),
    })?
}
