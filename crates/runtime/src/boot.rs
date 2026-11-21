//! The boot run of a script that snapshots, before any request.
//!
//! ```php
//! var_dump($_GET, $_SERVER['REQUEST_URI'] ?? 'none');   // in the boot run:
//! // array(0) {}  string(4) "none"
//! Phera\snapshot();
//! ```
//!
//! The boot runs the entry script as a request would, without the request:
//! `$_GET`, `$_POST`, `$_COOKIE`, `$_FILES` are empty, `$_SERVER` holds the
//! environment and what PHP's command line also sets without a request
//! (the script's paths and the start time), `php://input` is empty, and
//! what the script prints is not sent anywhere. A boot that reads a request
//! field gets nothing, which is the point: no request's values can end up
//! in the state every request starts from.

use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use phera_ext_http::{ResponseHead, Sapi};
use phera_vm::BootOutcome;

use crate::snapshots::{FileStamps, ScriptState};
use crate::{Runtime, WebWorker};

/// What a boot run did, for the server's log.
pub enum BootReport {
    /// The script called `Phera\snapshot()`; requests fork this state.
    Snapshot {
        elapsed: Duration,
        /// How many files the boot included.
        files: usize,
    },
    /// The script ended without calling it: requests run classically.
    Classic,
    /// The boot failed with this message; requests run classically and the
    /// boot is tried again later.
    Failed(String),
}

impl WebWorker {
    /// Boots `script` and keeps the result for this worker's requests.
    pub async fn boot(&self, script: &Path, document_root: &Path) -> BootReport {
        if !self.snapshots.start_boot(script) {
            return BootReport::Failed("a boot of this script is running already".into());
        }
        let started = Instant::now();
        let (state, report) = self
            .runtime
            .boot_script(self, script, document_root, started)
            .await;
        self.snapshots.finish_boot(script, state);
        report
    }

    /// Starts a boot in the background of this worker; requests are answered
    /// meanwhile (from the old snapshot, or classically).
    pub(crate) fn boot_in_background(&self, script: &Path, document_root: &Path) {
        if !self.background_boots {
            return;
        }
        let worker = self.clone();
        let (script, document_root) = (script.to_path_buf(), document_root.to_path_buf());
        tokio::task::spawn_local(async move {
            if let BootReport::Failed(message) = worker.boot(&script, &document_root).await {
                eprintln!(
                    "phera: worker {}: DEFECT: {} cannot be snapshotted, its requests run from the top: {message}",
                    worker.index,
                    script.display()
                );
            }
        });
    }
}

impl Runtime {
    async fn boot_script(
        &self,
        worker: &WebWorker,
        script: &Path,
        document_root: &Path,
        started: Instant,
    ) -> (ScriptState, BootReport) {
        let code = std::rc::Rc::new(crate::worker_code_cache::RequestCodeCache::new(
            worker.code.clone(),
        ));
        let unit = match self.entry_unit(&code, script).await {
            Ok(unit) => unit,
            Err(error) => return failed(error.to_string()),
        };
        let mut vm = self.new_vm(&worker.native, Sapi::Web, worker.environment.clone());
        vm.set_local_loader(code);
        if self.class_warmup {
            let sink = worker.snapshots.warmup_sink(script);
            let len = sink.borrow().len();
            if len > 0 {
                worker.snapshots.note_declared(script, len);
                vm.set_boot_warmup(sink);
            }
        }
        crate::http::define_boot_superglobals(
            &mut vm,
            script,
            document_root,
            worker.environment.as_ref().clone(),
        );
        let initial = crate::boot_changes::BootChanges::initial(&vm);
        vm.prepare_web_headers_with(ResponseHead::prepare, ResponseHead::set_error_status);
        phera_ext_streams::set_request(
            vm.state_mut(),
            script.parent().map(Path::to_path_buf),
            Vec::new().into(),
        );
        match vm.boot(unit).await {
            BootOutcome::Snapshot(snapshot) => {
                let files = snapshot.included_files();
                let count = files.len();
                let changes = crate::boot_changes::BootChanges::between(&initial, &snapshot);
                (
                    ScriptState::Ready(Rc::from(snapshot), FileStamps::of(files), Rc::new(changes)),
                    BootReport::Snapshot {
                        elapsed: started.elapsed(),
                        files: count,
                    },
                )
            }
            BootOutcome::Refused(message) => failed(message),
            BootOutcome::Finished(0) => (
                ScriptState::Classic(FileStamps::of(vm.included_files())),
                BootReport::Classic,
            ),
            BootOutcome::Finished(status) => {
                let output = String::from_utf8_lossy(&vm.output).trim().to_owned();
                failed(if output.is_empty() {
                    format!("the script exited with status {status}")
                } else {
                    output
                })
            }
        }
    }
}

fn failed(message: String) -> (ScriptState, BootReport) {
    (
        ScriptState::Failed(Instant::now()),
        BootReport::Failed(message),
    )
}
