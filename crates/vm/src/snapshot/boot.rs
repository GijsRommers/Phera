//! The boot run, the kept state, and requests that continue from it.
//!
//! ```php
//! require __DIR__.'/../vendor/autoload.php';
//! $app = require __DIR__.'/../bootstrap/app.php';   // the boot run does this
//! Phera\snapshot();                                   // ... and stops here
//! $app->handleRequest(Request::capture());            // every request does this
//! ```
//!
//! The runtime runs the boot with [`Vm::boot`], without a request. If the
//! script calls `Phera\snapshot()`, the result is a [`Snapshot`]; every
//! request is then [`Snapshot::fork`] followed by [`Vm::resume`], which
//! continues after the call with `true` as its result. A boot that reaches
//! the end of the script without the call tells the runtime that this
//! application does not snapshot.

use std::sync::Arc;

use phera_bytecode::CompiledUnit;
use phera_value::Value;

use super::SnapshotMode;
use crate::Vm;

/// The state of a booted application, kept by one worker thread.
pub struct Snapshot {
    /// Never run again: it is only copied.
    pub(crate) frozen: Vm,
}

/// How a boot run ended.
pub enum BootOutcome {
    /// The script called `Phera\snapshot()`.
    Snapshot(Snapshot),
    /// The script ended without calling it, with this exit status; what it
    /// printed is in [`Vm::output`].
    Finished(i32),
    /// The script called it, but the state could not be kept: the message
    /// of the `Phera\SnapshotException` it threw.
    Refused(String),
}

impl Vm {
    /// Registers how a fork gets the VM's own request state: all of it is
    /// caches, which a fork fills again.
    pub fn register_state_forks(registry: &mut phera_extension_api::Registry) {
        use phera_extension_api::fresh_state;
        registry.fork_state::<crate::reflection::AttributeCallSite>(fresh_state);
        registry.fork_state::<crate::reflection::initializers::InitializerCode>(fresh_state);
        registry.fork_state::<crate::serialization::property_index::PropertyCache>(fresh_state);
    }

    /// Sets what `Phera\snapshot()` does: nothing on the command line, keep
    /// the state in a boot run.
    pub fn set_snapshot_mode(&mut self, mode: SnapshotMode) {
        self.snapshot.mode = mode;
    }

    /// Whether this VM is a fork of a snapshot: [`Vm::resume`] runs it.
    pub fn is_fork(&self) -> bool {
        self.snapshot.mode == SnapshotMode::Forked
    }

    /// Every file this VM included, the entry script first.
    pub fn included_files(&self) -> Vec<String> {
        self.include_state.files().cloned().collect()
    }

    /// Whether a classic request called `Phera\snapshot()`.
    pub fn snapshot_called(&self) -> bool {
        self.snapshot.called
    }

    /// Runs the script without a request, until it calls `Phera\snapshot()`
    /// or ends.
    pub async fn boot(&mut self, unit: Arc<CompiledUnit>) -> BootOutcome {
        self.snapshot.mode = SnapshotMode::Boot;
        let status = self.run(unit).await;
        match (self.snapshot.taken.take(), self.snapshot.refused.take()) {
            (Some(snapshot), _) => BootOutcome::Snapshot(*snapshot),
            (None, Some(message)) => BootOutcome::Refused(message),
            (None, None) => BootOutcome::Finished(status),
        }
    }

    /// Continues a fork after `Phera\snapshot()` and runs the request to
    /// its end, as [`Vm::run`] does; returns the exit status.
    pub async fn resume(&mut self) -> i32 {
        if let Some(destination) = self.snapshot.resume {
            self.set(destination, Value::Bool(true));
        }
        self.execution_time
            .clone()
            .track(self.run_script_body())
            .await
    }
}

impl Snapshot {
    /// The state a request starts from: a copy that shares nothing a
    /// script can change with the snapshot or with other requests.
    pub fn fork(&self) -> Result<Vm, String> {
        self.frozen
            .fork_vm()
            .map_err(|refusal| format!("{}: {}", refusal.class, refusal.reason))
    }

    /// The value of a global variable at the call.
    pub fn global(&self, name: &str) -> Option<Value> {
        self.frozen.global(name)
    }

    /// Every file the boot run included, the entry script first: when one
    /// of them changes, the snapshot is out of date.
    pub fn included_files(&self) -> Vec<String> {
        self.frozen.include_state.files().cloned().collect()
    }
}
