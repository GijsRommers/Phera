//! The extra workers of a server: each is a thread with a VM of its own,
//! booted by running the same script up to its `listen()` call.
//!
//! PHP values live on `Rc`, so a VM never moves between threads. Worker 0's
//! routes are closures inside its VM; another thread gets the same routes
//! by running the same script, which registers them again in its own VM.
//! What crosses threads is plain data: the script's path and arguments, the
//! listening socket, and the shutdown signal.

use std::cell::RefCell;
use std::io::Write;
use std::net::TcpListener as StdListener;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use phera_vm::OutputSink;
use tokio::sync::watch;

use crate::Runtime;

/// What a command-line VM needs to run its script again on another thread.
#[derive(Clone)]
pub(crate) struct ScriptLaunch {
    pub(crate) runtime: Runtime,
    pub(crate) script: PathBuf,
    pub(crate) arguments: Vec<String>,
}

/// The [`ScriptLaunch`] of the script a command-line VM runs, kept in its
/// request state; empty in web requests, which have no server of this kind.
#[derive(Default)]
pub(crate) struct LaunchSlot(pub(crate) Option<ScriptLaunch>);

/// What makes a VM an extra worker of a server instead of a script that
/// starts one, kept in its request state until the script's
/// `new Server()` and `listen()` calls take it.
pub(crate) struct WorkerRole {
    pub(crate) index: usize,
    /// Taken by the constructor, which uses it instead of binding.
    pub(crate) listener: Option<StdListener>,
    pub(crate) stop: Arc<watch::Sender<bool>>,
    /// Switches the VM's output from quiet to live once `listen()` runs.
    pub(crate) output: WorkerOutput,
}

#[derive(Default)]
pub(crate) struct WorkerRoleSlot(pub(crate) Option<WorkerRole>);

/// The output of an extra worker: held back until the script reaches
/// `listen()` (it repeats what worker 0 printed), then written to standard
/// output like worker 0's. If the script ends before `listen()`, the output
/// held back explains why, so it goes to standard error.
#[derive(Clone, Default)]
pub(crate) struct WorkerOutput(Rc<RefCell<Option<Vec<u8>>>>);

impl WorkerOutput {
    fn quiet() -> WorkerOutput {
        WorkerOutput(Rc::new(RefCell::new(Some(Vec::new()))))
    }

    /// The script reached `listen()`: forget what it printed until now.
    pub(crate) fn go_live(&self) {
        self.0.borrow_mut().take();
    }

    /// What the script printed before it stopped short of `listen()`.
    fn held_back(&self) -> Option<Vec<u8>> {
        self.0.borrow_mut().take()
    }
}

impl OutputSink for WorkerOutput {
    fn write(&mut self, bytes: &[u8]) {
        match self.0.borrow_mut().as_mut() {
            Some(held) => held.extend_from_slice(bytes),
            None => {
                let mut stdout = std::io::stdout();
                let _ = stdout.write_all(bytes);
                let _ = stdout.flush();
            }
        }
    }

    fn flush(&mut self) {
        if self.0.borrow().is_none() {
            let _ = std::io::stdout().flush();
        }
    }
}

/// Starts worker `index` on a thread of its own: a runtime, a VM, and the
/// script run from the top. Its `listen()` serves on `listener`.
pub(crate) fn spawn_worker(
    launch: ScriptLaunch,
    index: usize,
    listener: StdListener,
    stop: Arc<watch::Sender<bool>>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name(format!("phera-http-{index}"))
        .spawn(move || run_worker(launch, index, listener, stop))
}

fn run_worker(
    launch: ScriptLaunch,
    index: usize,
    listener: StdListener,
    stop: Arc<watch::Sender<bool>>,
) {
    let output = WorkerOutput::quiet();
    let role = WorkerRole {
        index,
        listener: Some(listener),
        stop,
        output: output.clone(),
    };
    let ScriptLaunch {
        runtime,
        script,
        arguments,
    } = launch;
    let result = runtime.run_server_worker_blocking(&script, &arguments, output.clone(), role);
    if let Some(held_back) = output.held_back() {
        let mut stderr = std::io::stderr();
        let _ = writeln!(
            stderr,
            "phera: HTTP worker {index} stopped before reaching listen(); its output:"
        );
        let _ = stderr.write_all(&held_back);
        let _ = writeln!(stderr);
    }
    if let Err(error) = result {
        eprintln!("phera: HTTP worker {index} could not run {script:?}: {error}");
    }
}
