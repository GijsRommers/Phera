//! `chdir('/tmp')` changes path resolution for the current request.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use phera_extension_api::{Context, RequestState, Value, WorkingDirectory, value::Resource};

use crate::input::RequestInput;

pub struct Files {
    pub mask: u32,
    pub directory: Option<Rc<Resource>>,
    pub input: RequestInput,
    pub stream_contexts: HashMap<i64, Rc<Resource>>,
    resources: Vec<Weak<Resource>>,
    next_id: i64,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            mask: 0o022,
            directory: None,
            input: RequestInput::default(),
            stream_contexts: HashMap::new(),
            resources: Vec::new(),
            next_id: 5,
        }
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        // Script values can retain resources in cycles. Close those that outlive
        // ordinary reference counting before the next request runs.
        for resource in self.resources.iter().filter_map(Weak::upgrade) {
            if let Ok(mut data) = resource.data.try_borrow_mut() {
                data.take();
            }
        }
    }
}

pub fn files(context: &mut dyn Context) -> &mut Files {
    context.state().get_or_default()
}

pub fn set_request(state: &mut RequestState, cwd: Option<PathBuf>, input: Arc<[u8]>) {
    if let Some(cwd) = cwd {
        state.get_or_default::<WorkingDirectory>().set(cwd);
    }
    state.get_or_default::<Files>().input = RequestInput::new(input);
}

pub fn request_input(state: &RequestState) -> Option<&RequestInput> {
    state.get::<Files>().map(|files| &files.input)
}

pub fn new_resource(
    state: &mut RequestState,
    label: &'static str,
    data: impl std::any::Any,
) -> Value {
    let files = state.get_or_default::<Files>();
    let id = files.next_id;
    files.next_id += 1;
    let value = resource(id, label, data);
    files.track(&value);
    value
}

pub fn track_standard(state: &mut RequestState, value: &Value) {
    state.get_or_default::<Files>().track(value);
}

impl Files {
    fn track(&mut self, value: &Value) {
        if self.resources.len() >= 256 && self.next_id % 256 == 0 {
            self.resources
                .retain(|resource| resource.strong_count() > 0);
        }
        if let Value::Resource(resource) = value {
            self.resources.push(Rc::downgrade(resource));
        }
    }
}

pub fn resource(id: i64, label: &'static str, data: impl std::any::Any) -> Value {
    Value::Resource(Rc::new(Resource::new(id, label, Some(Box::new(data)))))
}

pub fn path(bytes: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(bytes))
}

pub fn bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

pub fn directory(context: &mut dyn Context) -> &mut WorkingDirectory {
    context.state().get_or_default()
}

pub fn search(
    directory: &WorkingDirectory,
    filename: &[u8],
    include: &phera_extension_api::PhpString,
    script: Option<&Path>,
) -> Option<PathBuf> {
    let name = path(filename);
    if filename.is_empty() {
        return None;
    }
    if name.is_absolute() || filename.starts_with(b"./") || filename.starts_with(b"../") {
        return canonical(&directory.resolve(name).ok()?).ok();
    }
    let components = include.as_bytes().split(|byte| *byte == b':');
    for component in components.filter(|component| !component.is_empty()) {
        let candidate = directory.resolve(&path(component).join(name)).ok()?;
        if let Ok(found) = canonical(&candidate) {
            return Some(found);
        }
    }
    let candidate = directory.resolve(&script?.parent()?.join(name)).ok()?;
    canonical(&candidate).ok()
}

/// On some platforms realpath accepts a trailing slash on a regular file.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    if bytes(path).ends_with(b"/") {
        std::fs::metadata(path)?;
    }
    path.canonicalize()
}

/// A fork of a snapshot keeps the umask and the resource numbering. An
/// open directory handle or a default stream context belongs to the boot,
/// and is refused; the request body (`php://input`) is the fork's own.
pub(crate) fn fork_files(
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Option<Box<dyn std::any::Any>> {
    let files = state.downcast_ref::<Files>()?;
    if files
        .directory
        .as_ref()
        .is_some_and(|directory| !directory.is_closed())
        || !files.stream_contexts.is_empty()
    {
        fork.refuse_state(
            "streams",
            "a directory handle or stream context opened before the call belongs to one request",
        );
    }
    let mut copy = Files::default();
    copy.mask = files.mask;
    copy.next_id = files.next_id;
    Some(Box::new(copy))
}
