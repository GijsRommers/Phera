//! PHP fibers switch heap stacks explicitly.
//!
//! ```php
//! $fiber = new Fiber(function () {
//!     $answer = array_map(fn () => Fiber::suspend('ready'), [0]);
//!     return $answer[0];
//! });
//! $fiber->start(); // "ready"
//! $fiber->resume('done'); // null, the fiber returned
//! $fiber->getReturn(); // "done"
//! ```
//!
//! The child interpreter future owns its PHP stack and the Rust futures
//! suspended inside it. The parent and child exchange request data only at
//! a switch. The shared slot is never borrowed while either future runs.
//! Generator activation and ordinary PHP call dispatch stay local to their
//! owning stack. A suspended fiber is closed by a graceful unwind at
//! request shutdown so its finally blocks can run.

mod class;
mod reflection;
mod stack;
mod switch;
mod trace;

use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use phera_extension_api::{NativeError, Registry, fiber::fiber_error};
use phera_value::{Object, Value};

use crate::calls::call_state::Callee;

pub(crate) use stack::Active;
pub(crate) use switch::GracefulClose;

pub(crate) type SuspendedFibers = BTreeMap<u32, Rc<Object>>;

pub(crate) fn register(registry: &mut Registry) {
    registry.class_in_module("Core", class::FIBER);
    registry.class_in_module("Core", class::FIBER_ERROR);
    registry.class_in_module("Reflection", reflection::CLASS);
}

struct Fiber {
    callable: Value,
    state: State,
}

enum State {
    Ready(Callee),
    Running,
    Suspended(stack::Suspended),
    Returned(Value),
    Failed,
    Exited,
}

fn current(active: &Option<Box<Active>>) -> Option<Rc<Object>> {
    active
        .as_ref()
        .and_then(|active| Weak::upgrade(&active.object))
}

fn with_state<T>(object: &Object, read: impl FnOnce(&mut Fiber) -> T) -> Result<T, NativeError> {
    let mut native = object
        .native
        .try_borrow_mut()
        .map_err(|_| internal_error!("fiber state borrowed"))?;
    let fiber = native
        .as_mut()
        .and_then(|state| state.downcast_mut::<Fiber>())
        .ok_or_else(|| fiber_error("Cannot start a fiber that has not been initialized"))?;
    Ok(read(fiber))
}

impl crate::Vm {
    pub(crate) fn current_fiber(&self) -> Option<Rc<Object>> {
        current(&self.active_fiber)
    }
}
