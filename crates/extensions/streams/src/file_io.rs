//! Blocking stream operations for native modules that hold a file across calls.
//!
//! For example, `session_start()` opens a private file on the blocking pool
//! and keeps its descriptor until `session_write_close()`.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use phera_extension_api::{Context, NativeError};

use crate::{io_policy::IoPolicy, request};

pub fn resolve(context: &mut dyn Context, filename: &[u8]) -> io::Result<PathBuf> {
    request::directory(context).resolve(request::path(filename))
}

pub async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, NativeError> {
    IoPolicy::Blocking.run(operation).await
}

/// Opens only a regular file, with no symlink traversal at the final component.
pub fn open_private_file(path: &Path, mode: u32, create: bool) -> io::Result<File> {
    use rustix::fs::OFlags;
    let flags = (OFlags::NOFOLLOW | OFlags::NONBLOCK).bits();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .mode(mode)
        .custom_flags(flags as i32)
        .open(path)?;
    let info = file.metadata()?;
    if !info.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a regular file",
        ));
    }
    Ok(file)
}

/// PHP accepts session files owned by the process user or by root.
pub fn file_is_owned(file: &File) -> io::Result<bool> {
    let uid = rustix::process::getuid().as_raw();
    let effective = rustix::process::geteuid().as_raw();
    let owner = file.metadata()?.uid();
    Ok(uid == 0 || owner == 0 || owner == uid || owner == effective)
}

/// Checks that a file opened before a lock wait still names the same inode.
pub fn same_file(file: &File, path: &Path) -> bool {
    if let Ok(opened) = file.metadata()
        && let Ok(current) = std::fs::symlink_metadata(path)
    {
        return opened.dev() == current.dev() && opened.ino() == current.ino();
    }
    false
}
