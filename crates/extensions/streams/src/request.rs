//! `chdir('/tmp')` changes path resolution for the current request.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use phera_extension_api::{Context, RequestState, Value, value::Resource};

pub struct Files {
    pub cwd: Option<PathBuf>,
    pub mask: u32,
    pub include_path: Vec<u8>,
    pub directory: Weak<Resource>,
    pub input: Arc<[u8]>,
    next_id: i64,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            cwd: env::current_dir().ok(),
            mask: 0o022,
            include_path: b".".to_vec(),
            directory: Weak::new(),
            input: Arc::default(),
            next_id: 4,
        }
    }
}

pub fn files(context: &mut dyn Context) -> &mut Files {
    context.state().get_or_default()
}

pub fn set_request(state: &mut RequestState, cwd: Option<PathBuf>, input: Arc<[u8]>) {
    let files = state.get_or_default::<Files>();
    if cwd.is_some() {
        files.cwd = cwd;
    }
    files.input = input;
}

pub fn new_resource(
    state: &mut RequestState,
    label: &'static str,
    data: impl std::any::Any,
) -> Value {
    let files = state.get_or_default::<Files>();
    let id = files.next_id;
    files.next_id += 1;
    resource(id, label, data)
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

impl Files {
    pub fn resolve(&self, name: &Path) -> io::Result<PathBuf> {
        if name.is_absolute() {
            return Ok(name.to_owned());
        }
        let cwd = self.cwd.as_ref().ok_or(io::ErrorKind::NotFound)?;
        Ok(cwd.join(name))
    }

    pub fn search(&self, filename: &[u8], script: Option<&Path>) -> Option<PathBuf> {
        let name = path(filename);
        if filename.is_empty() {
            return None;
        }
        if name.is_absolute() || filename.starts_with(b"./") || filename.starts_with(b"../") {
            return self.resolve(name).ok()?.canonicalize().ok();
        }
        for component in self.include_path.split(|byte| *byte == b':') {
            let candidate = self.resolve(&path(component).join(name)).ok()?;
            if let Ok(found) = candidate.canonicalize() {
                return Some(found);
            }
        }
        let candidate = self.resolve(&script?.parent()?.join(name)).ok()?;
        candidate.canonicalize().ok()
    }
}
