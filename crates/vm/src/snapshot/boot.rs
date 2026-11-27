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

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use foldhash::HashSet;
use phera_bytecode::CompiledUnit;
use phera_value::Value;

use super::SnapshotMode;
use crate::Vm;
use crate::classes::{Class, ClassTable};
use crate::code_table::CodeTable;
use crate::function_table::FunctionTable;

/// Maximum number of warm `CodeTable`/`ClassTable::local` pairs kept per
/// snapshot so steady-state and burst requests (up to the 32-VM worker cap)
/// skip cloning thousands of `FunctionFacts` + atomic `Arc`s while bounding
/// idle memory.
const MAX_RECYCLED_TABLES: usize = 32;

/// Immutable-world tables reused across clean forks of the same snapshot.
pub(crate) struct RecycledTables {
    pub(crate) code: CodeTable,
    pub(crate) local_classes: Vec<Option<Rc<Class>>>,
}

/// Carried by a forked `Vm` so its teardown can roll back and return tables
/// back to the originating snapshot's recycle pool. Uses a `Weak` handle so a
/// replaced snapshot frees immediately with zero cycles.
pub(crate) struct CodeRecycle {
    pool: Weak<RefCell<Vec<RecycledTables>>>,
    units_len: usize,
    functions_len: usize,
    trait_names: Rc<Vec<(u32, Rc<str>)>>,
    declared_classes: usize,
    local_classes_len: usize,
    aliases_len: usize,
    user_functions: usize,
}

impl CodeRecycle {
    pub(crate) fn try_return(
        self,
        code: &mut CodeTable,
        classes: &mut ClassTable,
        function_table: &FunctionTable,
    ) {
        let Some(pool) = self.pool.upgrade() else {
            return;
        };
        let Ok(mut pool) = pool.try_borrow_mut() else {
            return;
        };
        if pool.len() >= MAX_RECYCLED_TABLES {
            return;
        }
        if code.units_len() < self.units_len
            || code.functions_len() < self.functions_len
            || classes.declared_len() < self.declared_classes
            || classes.local_len() < self.local_classes_len
        {
            return;
        }
        let dirty_symbols_or_classes = classes.declared_len() != self.declared_classes
            || classes.aliases_len() != self.aliases_len
            || function_table.user_len() != self.user_functions;
        code.rollback_to_snapshot(
            self.units_len,
            self.functions_len,
            &self.trait_names,
            dirty_symbols_or_classes,
        );
        let placeholder = code.detached();
        let recycled_code = std::mem::replace(code, placeholder);
        let local_classes =
            classes.rollback_local_for_recycle(self.declared_classes, self.local_classes_len);
        pool.push(RecycledTables {
            code: recycled_code,
            local_classes,
        });
    }
}

/// The state of a booted application, kept by one worker thread.
pub struct Snapshot {
    /// Never run again: it is only copied.
    pub(crate) frozen: Vm,
    /// Warm `CodeTable` and `ClassTable::local` tables returned by clean
    /// finished forks of this snapshot.
    pub(crate) recycled: Rc<RefCell<Vec<RecycledTables>>>,
    /// Addresses of `Rc<PhpArray>`s in `frozen` proven transitively free of
    /// objects, references, and resources.
    pub(crate) pure_arrays: HashSet<usize>,
}

impl Snapshot {
    pub(crate) fn new(frozen: Vm, pure_arrays: HashSet<usize>) -> Self {
        Self {
            frozen,
            recycled: Rc::new(RefCell::new(Vec::new())),
            pure_arrays,
        }
    }
}

/// How a boot run ended.
pub enum BootOutcome {
    /// The script called `Phera\snapshot()`.
    Snapshot(Box<Snapshot>),
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
        registry.fork_state::<phera_extension_api::ProcessCancellation>(fresh_state);
        phera_extension_api::register_cycle_fork(registry);
        registry.fork_state::<crate::request_callbacks::ResourceFinalizers>(
            phera_extension_api::copy_state::<crate::request_callbacks::ResourceFinalizers>,
        );
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
        if self.debugger.is_some() {
            self.finish_debugger().await;
        }
        match (self.snapshot.taken.take(), self.snapshot.refused.take()) {
            (Some(snapshot), _) => BootOutcome::Snapshot(snapshot),
            (None, Some(message)) => BootOutcome::Refused(message),
            (None, None) => BootOutcome::Finished(status),
        }
    }

    /// Runs an entry script and returns its top-level return value without
    /// running shutdown/teardown so the caller can inspect the returned object
    /// (such as a `Phera\Http\Router`) and freeze a [`Snapshot`] of the VM.
    pub async fn boot_returning(&mut self, unit: Arc<CompiledUnit>) -> Result<Value, (i32, bool)> {
        if let Err(error) = self.load_entry_unit(&unit) {
            self.unwind(error);
            let status = self.end_script(crate::request_end::ScriptEnd::Fatal).await;
            return Err((status, false));
        }
        match self.interpret(0).await {
            Ok(value) => Ok(value),
            Err(error) => {
                if matches!(
                    &error,
                    phera_extension_api::NativeError::Unwind(payload)
                        if payload.0.is::<crate::request_end::Exit>()
                ) {
                    return Err((self.status, true));
                }
                let mut error = self.materialize_error(error);
                if let Err(failure) = self.run_destructors().await {
                    error = self.materialize_error(failure);
                }
                let end = crate::request_end::ScriptEnd::of(&error);
                self.report_uncaught(error).await;
                let status = self.end_script(end).await;
                Err((status, false))
            }
        }
    }

    /// Freezes the current idle VM state into a [`Snapshot`] that can be
    /// forked into isolated persistent instances.
    pub fn freeze_snapshot(&self) -> Result<Snapshot, String> {
        let (frozen, pure_arrays) = self
            .fork_vm_for_snapshot()
            .map_err(|refusal| format!("{}: {}", refusal.class, refusal.reason))?;
        Ok(Snapshot::new(frozen, pure_arrays))
    }

    /// Invokes a PHP callable directly on this VM (used by persistent router
    /// workers to dispatch matched requests without re-running an entry script).
    pub async fn call_callable(
        &mut self,
        callable: &Value,
        arguments: Vec<Value>,
    ) -> phera_extension_api::NativeResult {
        <Self as phera_extension_api::Context>::call(self, callable, arguments).await
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
        let recycled = self.recycled.borrow_mut().pop();
        let recycle = CodeRecycle {
            pool: Rc::downgrade(&self.recycled),
            units_len: self.frozen.code.units_len(),
            functions_len: self.frozen.code.functions_len(),
            trait_names: self.frozen.code.trait_method_names(),
            declared_classes: self.frozen.classes.declared_len(),
            local_classes_len: self.frozen.classes.local_len(),
            aliases_len: self.frozen.classes.aliases_len(),
            user_functions: self.frozen.function_table.user_len(),
        };
        self.frozen
            .fork_vm_with(recycled, Some(&self.pure_arrays), Some(recycle))
            .map(|(vm, _)| vm)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use phera_extension_api::Registry;

    use super::*;

    #[tokio::test]
    async fn snapshot_recycles_clean_tables_and_isolates_pure_arrays() {
        let source = r#"<?php
            define('APP_CFG', ['env' => 'prod', 'nested' => ['a' => 1, 'b' => 2]]);
            class Boxed {
                public const TAG = ['v' => 7];
                public array $cfg;
                public function __construct() {
                    $this->cfg = ['nested' => ['x' => 10]];
                }
            }
            $box = new Boxed();
            Phera\snapshot();
            $box->cfg['nested']['x'] += 5;
            echo $box->cfg['nested']['x'], ':', APP_CFG['nested']['a'], ':', Boxed::TAG['v'];
        "#;
        let unit = Arc::new(phera_compiler::compile(source, "recycle-clean.php").unwrap());
        let mut boot = Vm::new(Arc::new(Registry::new()));
        let BootOutcome::Snapshot(snapshot) = boot.boot(unit).await else {
            panic!("expected snapshot");
        };
        assert!(!snapshot.pure_arrays.is_empty());
        assert_eq!(snapshot.recycled.borrow().len(), 0);

        let mut first = snapshot.fork().unwrap();
        assert_eq!(first.resume().await, 0);
        assert_eq!(first.output, b"15:1:7");
        first.recycle();
        drop(first);
        assert_eq!(snapshot.recycled.borrow().len(), 1);

        let mut second = snapshot.fork().unwrap();
        assert_eq!(snapshot.recycled.borrow().len(), 0);
        assert_eq!(second.resume().await, 0);
        assert_eq!(second.output, b"15:1:7");
        second.recycle();
        drop(second);
        assert_eq!(snapshot.recycled.borrow().len(), 1);
    }

    #[tokio::test]
    async fn dirty_fork_rolls_back_and_isolates_recycled_tables() {
        let source = r#"<?php
            class Base {}
            Phera\snapshot();
            if (!empty($GLOBALS['DECLARE_EXTRA'])) {
                class RequestOnly {}
            }
            echo class_exists('RequestOnly', false) ? 'yes' : 'no';
        "#;
        let unit = Arc::new(phera_compiler::compile(source, "recycle-dirty.php").unwrap());
        let mut boot = Vm::new(Arc::new(Registry::new()));
        let BootOutcome::Snapshot(snapshot) = boot.boot(unit).await else {
            panic!("expected snapshot");
        };

        let mut dirty = snapshot.fork().unwrap();
        dirty.set_global("DECLARE_EXTRA", Value::Bool(true));
        assert_eq!(dirty.resume().await, 0);
        assert_eq!(dirty.output, b"yes");
        dirty.recycle();
        drop(dirty);
        assert_eq!(snapshot.recycled.borrow().len(), 1);

        // Next fork reuses the rolled-back tables and must not see `RequestOnly`.
        let mut clean = snapshot.fork().unwrap();
        assert_eq!(snapshot.recycled.borrow().len(), 0);
        assert_eq!(clean.resume().await, 0);
        assert_eq!(clean.output, b"no");
    }
}
