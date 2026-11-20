//! Calling a generator packages its frame. Resuming owns that frame until the next yield.
//!
//! ```php
//! function receive() { $reply = yield 'ready'; return $reply; }
//! $task = receive();
//! $task->send('done');
//! ```

mod activation;
pub(crate) mod class;
pub(crate) mod cleanup;
mod delegation;
mod execution;
mod in_place;
mod operations;
mod reflection;
mod resume;
pub(crate) use operations::local_observation;
pub(crate) mod trace;

use std::rc::{Rc, Weak};

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::{
    Vm,
    calls::{execution_loop::BatchExit, prepared_call::CallKind},
};
use activation::Activation;
use delegation::Delegate;

struct Generator {
    releases: cleanup::ReleaseQueue,
    /// The request's object pool: a released generator wakes the
    /// interpreter's destructor check, so that it is closed (and its
    /// variables destructed) before the next instruction.
    pool: Weak<phera_value::ObjectIds>,
    name: Value,
    function: crate::code_table::FunctionIndex,
    closure: Option<Rc<Object>>,
    reference: bool,
    state: State,
    leaf: Option<Weak<Object>>,
    item: Option<Item>,
    first_yield: bool,
    largest_key: i64,
}

enum State {
    Created(Activation),
    Ready(Activation),
    Yielded(Activation, Reg),
    Delegating(Activation, Reg, Delegate),
    Running,
    Returned(Value),
    Failed,
}

struct Item {
    key: Value,
    value: Value,
}

impl Generator {
    fn delegation_leaf(&self) -> Option<Rc<Object>> {
        if matches!(self.state, State::Delegating(_, _, Delegate::Generator(_))) {
            self.leaf.as_ref()?.upgrade()
        } else {
            None
        }
    }

    fn publish(&mut self, key: Option<Value>, value: Value) {
        let key = match key {
            Some(Value::Int(key)) => {
                self.largest_key = self.largest_key.max(key);
                Value::Int(key)
            }
            Some(key) => key,
            None => {
                self.largest_key = self.largest_key.wrapping_add(1);
                Value::Int(self.largest_key)
            }
        };
        self.item = Some(Item { key, value });
    }
}

pub(crate) enum Suspension {
    Yield {
        destination: Reg,
        key: Option<Value>,
        value: Value,
    },
    Delegate {
        destination: Reg,
        source: Value,
    },
}

impl Vm {
    #[cold]
    pub(crate) fn execute_generator(
        &mut self,
        instruction: Instruction,
        stop: usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        match instruction {
            Instruction::GeneratorStart => self.start_generator(stop),
            Instruction::Yield { .. } if self.force_closing_generator() => Err(NativeError::error(
                "Cannot yield from finally in a force-closed generator",
            )),
            Instruction::YieldFrom { .. } if self.force_closing_generator() => Err(
                NativeError::error("Cannot use \"yield from\" in a force-closed generator"),
            ),
            Instruction::Yield { dst, key, value } => {
                Ok(Some(self.generator_yield(dst, key, value)))
            }
            Instruction::YieldFrom { dst, src } => Ok(Some(self.generator_delegate(dst, src))),
            Instruction::GeneratorReturn { src } => {
                let Some(value) = self.read_defined_local(src) else {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                };
                self.finish_return_value(value, stop)
                    .map(|value| value.map(BatchExit::Returned))
            }
            _ => Err(internal_error!("generator instruction")),
        }
    }

    /// This is the first instruction after the call's argument prologue.
    #[cold]
    pub(crate) fn start_generator(
        &mut self,
        stop: usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        let class = self
            .classes
            .get("Generator")
            .ok_or_else(|| internal_error!("Generator class"))?;
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("generator frame"))?;
        let name = Value::from(frame.function().name.clone());
        let function = frame.frame.function;
        let closure = frame.closure().cloned();
        let destination = frame.frame.destination();
        let reference = frame.function().returns_reference;
        let constructed = if frame.frame.kind() == CallKind::Constructor {
            Some(
                frame
                    .this()
                    .cloned()
                    .ok_or_else(|| internal_error!("generator constructor receiver"))?,
            )
        } else {
            None
        };
        // The property operation receives the generator immediately. Its
        // continuation belongs to the caller, outside the detached activation.
        let continuation = if frame.frame.has_extras() && destination.is_none() {
            self.call_stack.current_extras_mut().property_hook.take()
        } else {
            None
        };
        let object = self.classes.allocate(class);
        let activation = self.suspend_generator_frame(activation::Storage::default())?;
        let first_generator = self.generator_releases.is_none();
        *object.native.borrow_mut() = Some(Box::new(Generator {
            releases: self.generator_release_queue(),
            pool: Rc::downgrade(&self.object_ids),
            name,
            function,
            closure,
            reference,
            state: State::Created(activation),
            leaf: None,
            item: None,
            first_yield: false,
            largest_key: -1,
        }));
        // new returns its receiver even when its constructor call returns a generator.
        let value = Value::Object(constructed.unwrap_or(object));
        if self.call_stack.depth() == stop {
            return Ok(Some(BatchExit::Returned(value)));
        }
        if let Some(destination) = destination {
            self.set(destination, value);
        } else if let Some(continuation) = continuation {
            self.returned_hook = Some(crate::property_hook_continuation::ReturnedHook {
                continuation,
                value,
            });
            self.pending_callbacks = true;
            return Ok(Some(BatchExit::Handlers));
        }
        Ok(first_generator.then_some(BatchExit::GeneratorTracking))
    }

    #[cold]
    pub(crate) fn generator_yield(
        &mut self,
        destination: Reg,
        key: Option<Reg>,
        value: Option<Reg>,
    ) -> BatchExit {
        let by_reference = self
            .current()
            .is_some_and(|frame| frame.function().returns_reference);
        let value = value.map_or(Value::Null, |source| {
            if by_reference {
                let value = self
                    .current_slot_mut(source)
                    .and_then(|slot| slot.clone())
                    .unwrap_or_default();
                if !value.is_reference() {
                    self.diagnostic(
                        "Notice",
                        "Only variable references should be yielded by reference".into(),
                    );
                }
                value
            } else {
                self.read_local(source)
            }
        });
        let key = key.map(|source| self.read_local(source));
        self.generator_suspension = Some(Suspension::Yield {
            destination,
            key,
            value,
        });
        BatchExit::GeneratorSuspended
    }

    /// Consumed operands may be scratch registers or named locals after optimization.
    pub(crate) fn discard_generator_temporary(&mut self, register: Reg) {
        if self
            .current()
            .is_some_and(|frame| register.index() >= frame.function().locals.len())
            && let Some(slot) = self.current_slot_mut(register)
        {
            *slot = None;
        }
    }

    #[cold]
    pub(crate) fn generator_delegate(&mut self, destination: Reg, source: Reg) -> BatchExit {
        let value = self.read_local(source);
        self.discard_generator_temporary(source);
        let source = value;
        self.generator_suspension = Some(Suspension::Delegate {
            destination,
            source,
        });
        BatchExit::GeneratorSuspended
    }
}

fn state<T>(object: &Object, act: impl FnOnce(&mut Generator) -> T) -> Result<T, NativeError> {
    let mut storage = object
        .native
        .try_borrow_mut()
        .map_err(|_| internal_error!("Generator storage borrowed"))?;
    let generator = storage
        .as_mut()
        .and_then(|state| state.downcast_mut::<Generator>())
        .ok_or_else(|| internal_error!("Generator storage"))?;
    Ok(act(generator))
}

pub(crate) fn generator_object(object: &Rc<Object>) -> bool {
    object.class.name() == "Generator"
}
