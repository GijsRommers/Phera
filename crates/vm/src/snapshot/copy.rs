//! The copy of a whole VM for a fork, field by field.
//!
//! ```php
//! $app = require 'bootstrap/app.php';   // objects, statics, autoloaders
//! Phera\snapshot();                       // copied into every fork
//! ```
//!
//! Each part of the VM is copied by its own `ForkCopy` (in its module), so
//! that a new field cannot be forgotten silently: [`Vm::fork_vm`] names
//! every field. What belongs to an operation in progress (a property hook,
//! a `__get` guard, a running generator or fiber) cannot be in a snapshot;
//! `Phera\snapshot()` refuses to be called there.

use std::rc::Rc;

use foldhash::HashSet;
use phera_value::{Fork, Value};

use super::natives::VmNatives;
use super::{CodeRecycle, ForkCopy, RecycledTables};
use crate::Vm;

/// Why the state could not be copied: the class (or kind of state), the
/// reason, and where the refused value was found.
#[derive(Debug, Clone)]
pub(crate) struct SnapshotRefusal {
    pub(crate) class: String,
    pub(crate) reason: String,
    /// The refused object's address in the original, to find its path.
    pub(crate) address: usize,
}

impl SnapshotRefusal {
    fn state(what: &str, reason: &str) -> Self {
        SnapshotRefusal {
            class: what.to_owned(),
            reason: reason.to_owned(),
            address: 0,
        }
    }
}

impl Vm {
    /// A copy of this VM used to freeze a snapshot, along with the addresses
    /// of transitively pure arrays that future forks can share in O(1).
    pub(crate) fn fork_vm_for_snapshot(&self) -> Result<(Vm, HashSet<usize>), SnapshotRefusal> {
        self.fork_vm_with(None, None, None)
    }

    /// A copy of this VM that shares nothing a script can change with it.
    pub(crate) fn fork_vm_with(
        &self,
        recycled: Option<RecycledTables>,
        pure_arrays: Option<&HashSet<usize>>,
        code_recycle: Option<CodeRecycle>,
    ) -> Result<(Vm, HashSet<usize>), SnapshotRefusal> {
        self.check_forkable()?;
        // Generators the boot made are released into the fork's own queue.
        let generator_releases = self.generator_releases.as_ref().map(|_| Rc::default());
        let natives = VmNatives {
            registry: &self.registry,
            rebind: Default::default(),
            releases: generator_releases
                .as_ref()
                .map(Rc::downgrade)
                .unwrap_or_default(),
        };
        let mut fork = Fork::with_snapshot_hints(
            self.classes.ids(),
            &natives,
            self.classes.objects.len(),
            pure_arrays,
        );
        let (recycled_code, recycled_local_classes) = match recycled {
            Some(tables) => (Some(tables.code), Some(tables.local_classes)),
            None => (None, None),
        };
        let globals = self.globals.fork_copy(&mut fork);
        let registers = self.registers.fork_copy(&mut fork);
        let call_stack = self.call_stack.fork_copy(&mut fork);
        let static_variables = self.static_variables.fork_copy(&mut fork);
        let class_states = self.class_states.fork_copy(&mut fork);
        let enum_cases = self.enum_cases.fork_copy(&mut fork);
        let constants = self.constants.fork_copy(&mut fork);
        let error_handlers = self.error_handlers.fork_copy(&mut fork);
        let request_callbacks = self.request_callbacks.fork_copy(&mut fork);
        let autoload = self.autoload.fork_copy(&mut fork);
        let output_writer = self.output_writer.fork_copy(&mut fork);
        let code = match recycled_code {
            Some(code) => code,
            None => self.code.fork_copy(&mut fork),
        };
        let unset_typed_properties = self.unset_typed_properties.fork_copy(&mut fork);
        let extension_values = self.extension_values.fork_copy(&mut fork);
        let websocket_resolver = self
            .websocket_resolver
            .as_ref()
            .map(|resolver| fork.value(resolver));
        // Extensions' state may look objects up by identity: every object
        // reachable from the VM is copied first.
        fork.drain();
        let state = self
            .state
            .fork(self.registry.state_forks(), &mut fork)
            .map_err(|name| {
                SnapshotRefusal::state(name, "this extension's request state cannot be copied")
            })?;
        fork.drain();
        let property_references = Rc::new(std::cell::RefCell::new(Default::default()));
        for (original, keys) in natives.rebind.take() {
            let Some(copy) = fork.copy_at(original) else {
                continue;
            };
            for key in keys {
                if let Some(Value::Reference(cell)) = copy.get(&key) {
                    crate::reference_holders::ReferenceHolders::track_object(
                        &property_references,
                        &copy,
                        &key,
                        &cell,
                    )
                    .map_err(|_| {
                        SnapshotRefusal::state(
                            "reference",
                            "a bound property could not be bound again",
                        )
                    })?;
                }
            }
        }
        let ids = fork.ids().clone();
        #[cfg(debug_assertions)]
        if std::env::var_os("PHERA_FORK_FOOTPRINT").is_some() {
            super::footprint::report(&fork, &code, &registers, &class_states);
        }
        let discovered_pure = if pure_arrays.is_none() {
            fork.pure_arrays_seen()
        } else {
            HashSet::default()
        };
        let objects = fork.finish_weak().map_err(|refusal| SnapshotRefusal {
            class: refusal.class,
            reason: refusal.reason,
            address: refusal.address,
        })?;
        let classes = self
            .classes
            .forked(ids.clone(), objects, recycled_local_classes);
        Ok((
            Vm {
                classes,
                object_ids: ids,
                enum_cases,
                native_callback_sites: Vec::new(),
                debug_objects: self.debug_objects.clone(),
                property_guards: Vec::new(),
                unset_typed_properties,
                extension_values,
                clone_scopes: Default::default(),
                closure_captures: Vec::new(),
                initializer_scope: None,
                initializer_class: None,
                argument_origin: self.argument_origin,
                native_called_class: None,
                pending_dimension: None,
                returned_hook: None,
                native_view_depth: None,
                prelinked_classes: self.prelinked_classes.clone(),
                function_table: self.function_table.forked(),
                code,
                class_states,
                globals,
                static_variables,
                diagnostic_reporter: crate::diagnostic_reporter::DiagnosticReporter::new(),
                diagnostic_location: None,
                include_state: self.include_state.forked(),
                call_stack,
                finally_stack: Vec::new(),
                generator_suspension: None,
                generator_traces: Vec::new(),
                generator_releases,
                draining_generators: false,
                closing_generator: None,
                executed_instructions: 0,
                registry: self.registry.clone(),
                output: Vec::new(),
                output_writer,
                request_callbacks,
                registers,
                constants,
                frame_limit: self.frame_limit,
                status: 0,
                error_handlers,
                pending_callbacks: false,
                failed_native: None,
                missed_native: None,
                state,
                reflection_cache: self.reflection_cache.clone(),
                property_references,
                autoload,
                request_end: self.request_end.clone(),
                cycles: self.cycles.clone(),
                ini_overrides: self.ini_overrides.clone(),
                execution_time: Rc::new(self.execution_time.forked()),
                active_fiber: None,
                fiber_bailout: false,
                suspended_fibers: Default::default(),
                snapshot: self.snapshot.forked(),
                websocket_sink: self.websocket_sink.clone(),
                websocket_resolver,
                websocket_is_event: false,
                code_recycle,
            },
            discovered_pure,
        ))
    }

    /// Refuses a copy in the middle of an operation that a fork could not
    /// finish: the operation's state lives in the Rust stack, not the VM.
    fn check_forkable(&self) -> Result<(), SnapshotRefusal> {
        // A built-in that runs its callbacks on the frame stack keeps its
        // state in `CallStack::native_loops`, which a fork does not copy.
        let busy =
            if self.call_stack.callback_depth() > 0 || !self.call_stack.native_loops.is_empty() {
                Some("inside a callback of a built-in function")
            } else if self.active_fiber.is_some() {
                Some("inside a Fiber")
            } else if self.generator_suspension.is_some() || self.closing_generator.is_some() {
                Some("inside a generator")
            } else if !self.finally_stack.is_empty() {
                Some("while a finally block runs")
            } else if !self.property_guards.is_empty() || self.returned_hook.is_some() {
                Some("inside a magic method or property hook")
            } else if self.initializer_scope.is_some() {
                Some("while a constant or default value is evaluated")
            } else if self
                .suspended_fibers
                .values()
                .any(|fiber| fiber.strong_count() > 0)
            {
                Some("while a Fiber is suspended")
            } else {
                None
            };
        match busy {
            Some(reason) => Err(SnapshotRefusal::state(
                "Phera\\snapshot()",
                &format!("cannot be called {reason}"),
            )),
            None => Ok(()),
        }
    }
}
