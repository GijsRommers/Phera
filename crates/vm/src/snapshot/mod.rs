//! `Phera\snapshot()`: boot once, then give every request a copy.
//!
//! ```php
//! // public/index.php
//! $kernel = new Kernel('prod', false);
//! $kernel->boot();              // runs once per worker, at server start
//! Phera\snapshot();             // the worker keeps the state as it is here
//! $response = $kernel->handle(Request::createFromGlobals());  // per request
//! ```
//!
//! `phera serve` runs the script once per worker before it accepts
//! connections, without a request. At the call the run stops and its whole
//! state is kept as the worker's snapshot: globals, the call stack and its
//! registers, static properties, classes, autoloaders, handlers. Each
//! request then starts as a fork of that state and continues after the
//! call, with its own superglobals.
//!
//! A fork is a copy ([`Vm::fork_vm`]): every object and reference cell the
//! snapshot can reach is copied once (see `phera_value::Fork`), so no
//! request can see what another one does, and the snapshot never changes.
//! Linked classes are immutable and shared; what a request may change about
//! them is in its own tables (see `crate::classes::class_state`).

mod boot;
mod callees;
mod copy;
#[cfg(debug_assertions)]
mod footprint;
mod intrinsic;
mod natives;
mod paths;

use phera_value::Fork;

use crate::Vm;

pub use boot::{BootOutcome, Snapshot};
pub(crate) use copy::SnapshotRefusal;

/// What `Phera\snapshot()` does in this VM.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SnapshotMode {
    /// On the command line: returns `false`.
    #[default]
    Off,
    /// A request of `phera serve` that runs the script from the top: returns
    /// `true` and notes the call, so that the worker can boot the script.
    Classic,
    /// The boot run of `phera serve`: the state is kept, and the run stops.
    Boot,
    /// A fork of a snapshot: the snapshot is taken; returns `true`.
    Forked,
}

/// The classes a worker's requests autoloaded, shared with every fork, so
/// that a re-boot can declare them in the snapshot (see
/// `crate::snapshot::intrinsic`). Names without a leading backslash.
pub type WarmupSink = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// The VM's part in snapshots: its mode, and in a boot run the state kept
/// at the call.
#[derive(Default)]
pub(crate) struct SnapshotState {
    pub(crate) mode: SnapshotMode,
    /// The register that receives the call's result when a fork resumes.
    pub(crate) resume: Option<phera_bytecode::Reg>,
    /// The kept state, once a boot run reached the call.
    pub(crate) taken: Option<Box<Snapshot>>,
    /// The script called `Phera\snapshot()` in a classic request.
    pub(crate) called: bool,
    /// Why the call could not keep the state, in a boot run. The script
    /// may catch the exception, or its own handler may hide it.
    pub(crate) refused: Option<String>,
    /// A fork adds here every class a request's autoloader call produced.
    pub(crate) warmup_sink: Option<WarmupSink>,
    /// A boot run declares the classes in this list through the autoloaders
    /// before the call keeps the state; a class whose load fails is
    /// removed from the list.
    pub(crate) boot_warmup: Option<WarmupSink>,
}

impl Vm {
    /// A fork adds to `sink` every class that a request's autoloader call
    /// produced (see [`SnapshotState::warmup_sink`]).
    pub fn set_warmup_sink(&mut self, sink: WarmupSink) {
        self.snapshot.warmup_sink = Some(sink);
    }

    /// A boot run declares the classes in `sink` through the registered
    /// autoloaders just before `Phera\snapshot()` keeps the state; a class
    /// whose load fails or throws is removed from `sink`.
    pub fn set_boot_warmup(&mut self, sink: WarmupSink) {
        self.snapshot.boot_warmup = Some(sink);
    }
}

impl SnapshotState {
    pub(crate) fn forked(&self) -> Self {
        SnapshotState {
            mode: self.mode,
            resume: self.resume,
            taken: None,
            called: false,
            refused: None,
            warmup_sink: None,
            boot_warmup: None,
        }
    }
}

/// A part of the VM's state that a fork copies: every PHP value inside goes
/// through the [`Fork`], everything else is cloned.
pub(crate) trait ForkCopy {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self;
}

impl ForkCopy for phera_value::Value {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        fork.value(self)
    }
}

impl ForkCopy for std::rc::Rc<phera_value::Object> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        fork.object(self)
    }
}

impl ForkCopy for std::rc::Rc<phera_value::PhpArray> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        fork.array(self)
    }
}

impl<T: ForkCopy> ForkCopy for Option<T> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        self.as_ref().map(|value| value.fork_copy(fork))
    }
}

impl<T: ForkCopy> ForkCopy for Vec<T> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        self.iter().map(|value| value.fork_copy(fork)).collect()
    }
}

impl<T: ForkCopy> ForkCopy for std::collections::VecDeque<T> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        self.iter().map(|value| value.fork_copy(fork)).collect()
    }
}

/// Globals keep their order of declaration, so the copy keeps it too.
impl<K: Clone + Eq + std::hash::Hash, V: ForkCopy> ForkCopy for indexmap::IndexMap<K, V> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        self.iter()
            .map(|(key, value)| (key.clone(), value.fork_copy(fork)))
            .collect()
    }
}

impl<K: Clone + Eq + std::hash::Hash, V: ForkCopy> ForkCopy for std::collections::HashMap<K, V> {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        self.iter()
            .map(|(key, value)| (key.clone(), value.fork_copy(fork)))
            .collect()
    }
}
