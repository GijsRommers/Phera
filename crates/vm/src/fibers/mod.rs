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

mod caller;
mod class;
mod globals;
mod lifecycle;
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

/// The suspended fibers, by object id. They are held weakly: a fiber whose
/// last reference goes is destroyed at that moment, like an object with a
/// destructor (see `Vm::close_fiber`).
pub(crate) type SuspendedFibers = BTreeMap<u32, Weak<Object>>;

pub(crate) fn register(registry: &mut Registry) {
    registry.class_in_module("Core", class::FIBER);
    registry.class_in_module("Core", class::FIBER_ERROR);
    registry.class_in_module("Reflection", reflection::CLASS);
}

struct Fiber {
    callable: Value,
    live: Option<Weak<stack::Exchange>>,
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

/// The fork of a fiber the boot made, for [`crate::snapshot`]: `None` when
/// `state` is not a fiber's. One not started yet starts in each fork, and
/// a finished one keeps its result. A suspended fiber's stack is a VM of
/// its own, which the boot cannot be stopped inside: it is refused when
/// the snapshot is taken.
pub(crate) fn fork_fiber(
    state: &dyn std::any::Any,
    fork: &mut phera_value::Fork<'_>,
) -> Option<Result<Box<dyn std::any::Any>, String>> {
    use crate::snapshot::ForkCopy;
    let fiber = state.downcast_ref::<Fiber>()?;
    let state = match &fiber.state {
        State::Ready(callee) => State::Ready(callee.fork_copy(fork)),
        State::Returned(value) => State::Returned(fork.value(value)),
        State::Failed => State::Failed,
        State::Exited => State::Exited,
        State::Running | State::Suspended(_) => {
            return Some(Err(
                "it is suspended; let it finish before Phera\\snapshot()".into(),
            ));
        }
    };
    Some(Ok(Box::new(Fiber {
        callable: fork.value(&fiber.callable),
        live: None,
        state,
    })))
}

fn current(active: &Option<Box<Active>>) -> Option<Rc<Object>> {
    active.as_ref().and_then(|active| active.object.upgrade())
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
