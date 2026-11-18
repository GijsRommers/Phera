//! `is_uploaded_file()` and `move_uploaded_file()`.
//!
//! An uploaded file waits in a temporary file until the script moves it:
//!
//! ```php
//! $upload = $_FILES['photo'];
//! if ($upload['error'] === UPLOAD_ERR_OK) {
//!     move_uploaded_file($upload['tmp_name'], "/data/photos/{$id}.jpg");
//! }
//! ```
//!
//! Both functions accept only a path this very request received as an
//! upload. That is their point: a script that is tricked into passing
//! `/etc/passwd` as the "uploaded" file moves nothing. What the script does
//! not move is deleted when the request ends.

use std::path::{Path, PathBuf};

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, WorkingDirectory, args,
};

/// The temporary files of the uploads this request received and has not
/// moved yet.
#[derive(Default)]
pub struct UploadedFiles {
    waiting: Vec<PathBuf>,
}

impl UploadedFiles {
    pub fn new(waiting: Vec<PathBuf>) -> UploadedFiles {
        UploadedFiles { waiting }
    }

    fn holds(&self, path: &Path) -> bool {
        self.waiting.iter().any(|waiting| waiting == path)
    }

    fn forget(&mut self, path: &Path) {
        self.waiting.retain(|waiting| waiting != path);
    }

    /// The files nobody moved. The runtime deletes them after the request.
    pub fn into_unclaimed(self) -> Vec<PathBuf> {
        self.waiting
    }
}

const IS_UPLOADED_FILE: &[Param] = &[Param::new("filename", ParamType::String)];
const MOVE_UPLOADED_FILE: &[Param] = &[
    Param::new("from", ParamType::String),
    Param::new("to", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "is_uploaded_file",
        IS_UPLOADED_FILE,
        is_uploaded_file,
    ));
    registry.function(FunctionEntry::sync(
        "move_uploaded_file",
        MOVE_UPLOADED_FILE,
        move_uploaded_file,
    ));
}

fn path_argument(arguments: &[Value], index: usize) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(args::bytes(arguments, index)).into_owned())
}

fn is_uploaded_file(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let path = path_argument(arguments, 0);
    let uploads = context.state().get_or_default::<UploadedFiles>();
    Ok(Value::Bool(uploads.holds(&path)))
}

fn move_uploaded_file(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let from = path_argument(arguments, 0);
    let to = path_argument(arguments, 1);
    if !context
        .state()
        .get_or_default::<UploadedFiles>()
        .holds(&from)
    {
        return Ok(Value::Bool(false));
    }
    let destination = context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(&to);
    let moved = destination.and_then(|destination| move_file(&from, &destination));
    match moved {
        Ok(()) => {
            context
                .state()
                .get_or_default::<UploadedFiles>()
                .forget(&from);
            Ok(Value::Bool(true))
        }
        Err(error) => {
            context.warning(format!(
                "move_uploaded_file(): Unable to move \"{}\" to \"{}\": {}",
                from.display(),
                to.display(),
                describe(&error)
            ));
            Ok(Value::Bool(false))
        }
    }
}

/// Renames the file, or copies and deletes it when the destination is on
/// another file system. The result is readable like any file the script
/// creates, not private like the temporary file.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_err() {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)?;
    }
    make_ordinary(to)
}

#[cfg(unix)]
fn make_ordinary(file: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // 0666 without the usual umask of 022.
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
fn make_ordinary(_file: &Path) -> std::io::Result<()> {
    Ok(())
}

/// "No such file or directory", without Rust's "(os error 2)".
fn describe(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error") {
        Some(end) => text[..end].to_string(),
        None => text,
    }
}
