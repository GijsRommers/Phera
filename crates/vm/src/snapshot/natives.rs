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

/// The VM's [`NativeFork`]. It remembers the objects whose properties are
/// bound to references: the fork binds them again in its own table.
pub(super) struct VmNatives<'r> {
    pub(super) registry: &'r phera_extension_api::Registry,
    pub(super) rebind: std::cell::RefCell<Vec<(usize, Vec<phera_value::ArrayKey>)>>,
    /// The fork's queue of released generators.
    pub(super) releases: crate::generators::cleanup::ReleaseQueue,
}

impl NativeFork for VmNatives<'_> {
    fn fork_resource(
        &self,
        resource: &phera_value::Resource,
        fork: &mut Fork<'_>,
    ) -> Result<phera_value::Resource, String> {
        let hook = self
            .registry
            .resource_fork(resource.kind)
            .ok_or("an open resource of this kind belongs to one request")?;
        let data = resource
            .data
            .try_borrow()
            .map_err(|_| "the resource is in use".to_owned())?;
        let copy = match data.as_deref() {
            Some(data) => Some(hook(data, fork)?),
            None => None,
        };
        Ok(phera_value::Resource::new(resource.id, resource.kind, copy))
    }

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
        if let Some(generator) = crate::generators::fork_generator(state, fork, &self.releases) {
            return generator;
        }
        if let Some(fiber) = crate::fibers::fork_fiber(state, fork) {
            return fiber;
        }
        if let Some(keys) = crate::reference_holders::bound_properties(state) {
            let address = std::ptr::from_ref(object) as usize;
            self.rebind.borrow_mut().push((address, keys));
            return Ok(crate::reference_holders::empty_bookkeeping());
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
