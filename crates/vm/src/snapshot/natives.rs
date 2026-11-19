//! Native state in a fork: the VM's own kinds, then each class's hook.
//!
//! ```php
//! $double = fn (int $n) => $n * 2;   // a Closure: its captures are copied
//! $stack = new SplStack;             // SPL's hook copies its storage
//! $file = fopen('/tmp/log', 'a');    // a resource: refused
//! Phera\snapshot();
//! ```
//!
//! The VM keeps state of its own on some objects (closures, the prepared
//! call of a lazy object). Every other native state belongs to the
//! built-in class that made it, which copies it through
//! `ClassHooks::fork`. A class without a hook cannot be in a snapshot.

use std::any::Any;

use phera_value::{Fork, NativeFork, Object};

use super::ForkCopy;
use crate::calls::call_state::Callee;
use crate::classes::Class;
use crate::closures::ClosureState;

/// The VM's [`NativeFork`].
pub(super) struct VmNatives;

impl NativeFork for VmNatives {
    fn fork_native(
        &self,
        object: &Object,
        state: &dyn Any,
        fork: &mut Fork<'_>,
    ) -> Result<Box<dyn Any>, String> {
        if let Some(closure) = state.downcast_ref::<ClosureState>() {
            return Ok(Box::new(closure.fork_copy(fork)));
        }
        // A lazy object's initializer, resolved when the object was made.
        if let Some(callee) = state.downcast_ref::<Callee>() {
            return Ok(Box::new(callee.fork_copy(fork)));
        }
        if crate::reference_holders::is_reference_bookkeeping(state) {
            return Err("a property of it is bound to a PHP reference".into());
        }
        let hook = object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .and_then(|class| class.hooks.as_ref())
            .and_then(|hooks| hooks.fork);
        match hook {
            Some(fork_state) => fork_state(object, state, fork),
            None => Err("its built-in state cannot be copied".into()),
        }
    }
}
