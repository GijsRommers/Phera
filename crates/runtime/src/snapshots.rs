//! A web worker's snapshots: one booted state per entry script.
//!
//! ```php
//! // public/index.php
//! $app = require __DIR__.'/../bootstrap/app.php';
//! Phera\snapshot();          // the boot run stops here, at server start
//! $app->handleRequest(Request::capture());
//! ```
//!
//! Each worker boots such a script once, before it accepts connections and
//! without a request (see [`crate::Runtime::boot_superglobals`]), and keeps
//! the state at the call. Every request is a fork of it. A script whose
//! boot ends without the call is remembered as classic, and a boot that
//! fails is reported and retried later; both are answered from the top.
//!
//! The snapshot is out of date when a file the boot included changes. Files
//! are checked as opcache checks them (`opcache.validate_timestamps`, every
//! `opcache.revalidate_freq` seconds, 2 by default). A change starts a new
//! boot in the background of the worker; requests keep forking the old
//! snapshot until the new one is ready.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use phera_vm::{IncludeCachePolicy, Snapshot};

/// What a worker knows about one entry script.
pub(crate) enum ScriptState {
    /// Booted: requests fork this, and make the boot's changes to their
    /// superglobals again.
    Ready(
        Rc<Snapshot>,
        FileStamps,
        Rc<crate::boot_changes::BootChanges>,
    ),
    /// The boot ended without `Phera\snapshot()`: requests run classically
    /// until one of these files changes.
    Classic(FileStamps),
    /// The boot failed; it is tried again after this moment.
    Failed(Instant),
}

/// The files a boot included, with their modification times then.
pub(crate) struct FileStamps {
    files: Vec<(PathBuf, Option<SystemTime>)>,
    /// When the files were last compared with the disk.
    checked: Instant,
}

impl FileStamps {
    pub(crate) fn of(files: Vec<String>) -> Self {
        let files = files
            .into_iter()
            .map(PathBuf::from)
            .map(|file| {
                let modified = modified(&file);
                (file, modified)
            })
            .collect();
        FileStamps {
            files,
            checked: Instant::now(),
        }
    }

    /// Whether some file changed. Looks at the disk only once per interval.
    fn changed(&mut self, policy: IncludeCachePolicy) -> bool {
        if !policy.validate_timestamps || self.checked.elapsed() < policy.revalidate_freq {
            return false;
        }
        self.checked = Instant::now();
        self.files
            .iter()
            .any(|(file, stamp)| modified(file) != *stamp)
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .ok()
}

/// How long a failed boot waits before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(2);

/// What requests do while a snapshot whose files changed boots again.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stale {
    /// Production: fork the old snapshot until the new one is ready, as
    /// opcache serves the old code until it has compiled the new.
    ForkOld,
    /// `phera serve --dev`: run from the top, so that the next request after
    /// an edit already runs the edited code.
    RunFromTheTop,
}

/// What a request should do with its script.
pub(crate) enum Plan {
    /// Fork this snapshot.
    Fork(Rc<Snapshot>, Rc<crate::boot_changes::BootChanges>),
    /// Run the script from the top.
    Classic,
}

/// The snapshots of one worker thread.
#[derive(Default)]
pub(crate) struct WorkerSnapshots {
    scripts: RefCell<HashMap<PathBuf, ScriptState>>,
    /// Scripts whose boot is running now.
    booting: RefCell<Vec<PathBuf>>,
}

impl WorkerSnapshots {
    /// What to do for a request to `script`, and whether a boot should be
    /// started for it (the snapshot is out of date, a failed boot is due
    /// again, or a classic script's files changed).
    pub(crate) fn plan(&self, script: &Path, policy: IncludeCachePolicy, stale: Stale) -> (Plan, bool) {
        let mut scripts = self.scripts.borrow_mut();
        let booting = self.booting.borrow().iter().any(|path| path == script);
        match scripts.get_mut(script) {
            Some(ScriptState::Ready(snapshot, files, changes)) => {
                let changed = !booting && files.changed(policy);
                if changed && stale == Stale::RunFromTheTop {
                    scripts.remove(script);
                    return (Plan::Classic, true);
                }
                (Plan::Fork(snapshot.clone(), changes.clone()), changed)
            }
            Some(ScriptState::Classic(files)) => {
                let stale = !booting && files.changed(policy);
                if stale {
                    scripts.remove(script);
                }
                (Plan::Classic, stale)
            }
            Some(ScriptState::Failed(at)) => {
                (Plan::Classic, !booting && at.elapsed() >= RETRY_AFTER)
            }
            None => (Plan::Classic, false),
        }
    }

    /// Whether this worker knows the script (booted it, or tried).
    pub(crate) fn knows(&self, script: &Path) -> bool {
        self.scripts.borrow().contains_key(script)
            || self.booting.borrow().iter().any(|path| path == script)
    }

    /// Marks a boot as running; false if one runs already.
    pub(crate) fn start_boot(&self, script: &Path) -> bool {
        let mut booting = self.booting.borrow_mut();
        if booting.iter().any(|path| path == script) {
            return false;
        }
        booting.push(script.to_path_buf());
        true
    }

    /// The result of a boot replaces what was there.
    pub(crate) fn finish_boot(&self, script: &Path, state: ScriptState) {
        self.booting.borrow_mut().retain(|path| path != script);
        self.scripts
            .borrow_mut()
            .insert(script.to_path_buf(), state);
    }
}

/// Where `phera serve` remembers that a script calls `Phera\snapshot()`, so
/// that the next start boots it before the first request: one small file
/// per script under the user's cache directory, named after the script's
/// path. Losing it costs one classic request at the next start.
fn marker(script: &Path) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    // FNV-1a: stable across builds, unlike the standard library's hasher.
    let hash = script
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    Some(
        base.join("phera")
            .join("snapshots")
            .join(format!("{hash:016x}")),
    )
}

/// Whether an earlier start found that `script` calls `Phera\snapshot()`.
pub fn remembered(script: &Path) -> bool {
    marker(script).is_some_and(|marker| {
        std::fs::read(marker).is_ok_and(|content| content == script.as_os_str().as_encoded_bytes())
    })
}

/// Remembers that `script` calls `Phera\snapshot()`; best effort.
pub(crate) fn remember(script: &Path) {
    if let Some(marker) = marker(script)
        && let Some(directory) = marker.parent()
        && std::fs::create_dir_all(directory).is_ok()
    {
        let _ = std::fs::write(marker, script.as_os_str().as_encoded_bytes());
    }
}
