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
mod resume;
pub(crate) use resume::local_observation;
pub(crate) mod trace;

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::{Vm, calls::execution_loop::BatchExit};
use activation::Activation;
use delegation::Delegate;

struct Generator {
    releases: cleanup::ReleaseQueue,
    name: Value,
    reference: bool,
    state: State,
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
        let destination = frame.frame.destination();
        let reference = frame.function().returns_reference;
        let object = self.classes.allocate(class);
        let activation = self.suspend_generator_frame(activation::Storage::default())?;
        *object.native.borrow_mut() = Some(Box::new(Generator {
            releases: self.generator_release_queue(),
            name,
            reference,
            state: State::Created(activation),
            item: None,
            first_yield: false,
            largest_key: -1,
        }));
        let value = Value::Object(object);
        if self.call_stack.depth() == stop {
            return Ok(Some(BatchExit::Returned(value)));
        }
        if let Some(destination) = destination {
            self.set(destination, value);
        }
        Ok(None)
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
