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

use std::cell::{Cell, RefCell};
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
    pub(crate) fn changed(&mut self, policy: IncludeCachePolicy) -> bool {
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

/// The worker boots again to declare the collected classes in the snapshot
/// after this many forked requests without a new class in the list. At the
/// gate's 40–200 requests per second that is one to six seconds; a route hit
/// once an hour still lands in the list and is declared at the next settle.
const WARMUP_SETTLE_REQUESTS: u64 = 64;

/// At most this many warm-up boots per script per worker, so a site with
/// thousands of routes does not boot forever. Boots that a changed file
/// starts do not count against this.
const WARMUP_MAX_BOOTS: u32 = 3;

/// The classes this worker's requests autoloaded, and how long ago the
/// list last grew.
#[derive(Default)]
pub(crate) struct Warmup {
    pub(crate) classes: Rc<RefCell<Vec<String>>>,
    requests_since_growth: Cell<u64>,
    /// How many warm-up boots ran, and the length of the list the last
    /// boot that used it declared.
    boots: Cell<u32>,
    declared_len: Cell<usize>,
}

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
    /// Per script, the classes its requests autoloaded.
    warmup: RefCell<HashMap<PathBuf, Warmup>>,
}

impl WorkerSnapshots {
    /// What to do for a request to `script`, and whether a boot should be
    /// started for it (the snapshot is out of date, a failed boot is due
    /// again, or a classic script's files changed).
    pub(crate) fn plan(
        &self,
        script: &Path,
        policy: IncludeCachePolicy,
        stale: Stale,
    ) -> (Plan, bool) {
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
                count_from_the_top();
                (Plan::Classic, !booting && at.elapsed() >= RETRY_AFTER)
            }
            None => (Plan::Classic, false),
        }
    }

    /// The snapshot of `script`, if its boot finished with one.
    pub(crate) fn snapshot(&self, script: &Path) -> Option<Rc<Snapshot>> {
        match self.scripts.borrow().get(script) {
            Some(ScriptState::Ready(snapshot, ..)) => Some(snapshot.clone()),
            _ => None,
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

    /// The sink a fork of `script` records the classes its requests
    /// autoload into.
    pub(crate) fn warmup_sink(&self, script: &Path) -> phera_vm::WarmupSink {
        self.warmup
            .borrow_mut()
            .entry(script.to_path_buf())
            .or_default()
            .classes
            .clone()
    }

    /// The classes collected for `script` so far.
    pub(crate) fn warmup_classes(&self, script: &Path) -> Vec<String> {
        self.warmup
            .borrow()
            .get(script)
            .map(|warmup| warmup.classes.borrow().clone())
            .unwrap_or_default()
    }

    /// How many warm-up boots ran for `script` (a boot a changed file
    /// started does not count).
    pub(crate) fn warmup_boots(&self, script: &Path) -> u32 {
        self.warmup
            .borrow()
            .get(script)
            .map(|warmup| warmup.boots.get())
            .unwrap_or(0)
    }

    /// Records that a boot of `script` declared `len` classes from the
    /// warm-up list: the length the last boot declared.
    pub(crate) fn note_declared(&self, script: &Path, len: usize) {
        let mut warmup = self.warmup.borrow_mut();
        let Some(entry) = warmup.get_mut(script) else {
            return;
        };
        if len > entry.declared_len.get() {
            entry.declared_len.set(len);
        }
        entry.requests_since_growth.set(0);
    }

    /// Records that a forked request of `script` ran; `grew` says it
    /// autoloaded a class not in the list yet. Returns true when the
    /// worker should boot again to declare the list.
    ///
    /// A warm-up boot starts only when the list has grown since the last
    /// boot that used it (its length is `declared_len`) and has then been
    /// stable for WARMUP_SETTLE_REQUESTS requests. A settled worker whose
    /// requests load nothing new therefore never boots again.
    pub(crate) fn note_forked_request(&self, script: &Path, grew: bool) -> bool {
        let booting = self.booting.borrow().iter().any(|path| path == script);
        let mut warmup = self.warmup.borrow_mut();
        let Some(entry) = warmup.get_mut(script) else {
            return false;
        };
        if grew {
            entry.requests_since_growth.set(0);
        } else {
            entry
                .requests_since_growth
                .set(entry.requests_since_growth.get() + 1);
        }
        let len = entry.classes.borrow().len();
        if len > entry.declared_len.get()
            && entry.requests_since_growth.get() >= WARMUP_SETTLE_REQUESTS
            && entry.boots.get() < WARMUP_MAX_BOOTS
            && !booting
        {
            entry.boots.set(entry.boots.get() + 1);
            entry.declared_len.set(len);
            entry.requests_since_growth.set(0);
            return true;
        }
        false
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

/// Requests of this process that ran from the top although their script
/// snapshots: its boot failed or was refused, or a fork could not be made.
/// Snapshots are the only mode; each of these is a defect to fix, which the
/// server's log names and `--health` reports (`Phera-From-The-Top`).
static FROM_THE_TOP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn count_from_the_top() {
    FROM_THE_TOP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// How many requests ran from the top so far (see [`count_from_the_top`]).
pub fn requests_from_the_top() -> u64 {
    FROM_THE_TOP.load(std::sync::atomic::Ordering::Relaxed)
}
