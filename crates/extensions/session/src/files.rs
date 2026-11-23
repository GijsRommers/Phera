//! `SessionHandler::read($id)` and `session_start()` use the same file backend.

use phera_ext_streams::file_io::blocking;
use phera_extension_api::{Context, NativeError, PhpString};

use crate::{
    file_handler::{Directory, FileError, LockedFile},
    state::{state, warning},
};

pub struct Files {
    pub directory: Directory,
    locked: Option<(PhpString, LockedFile)>,
}

pub fn open(context: &mut dyn Context, path: &PhpString) -> Result<(), String> {
    let directory = Directory::from_setting(context, path)?;
    state(context).files = Some(Files {
        directory,
        locked: None,
    });
    Ok(())
}

pub async fn close(context: &mut dyn Context) -> Result<(), NativeError> {
    if let Some((_, file)) = state(context).files.take().and_then(|files| files.locked) {
        blocking(move || drop(file)).await?;
    }
    Ok(())
}

pub async fn read(
    context: &mut dyn Context,
    id: &PhpString,
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    let Some(file) = lock(context, id, function).await? else {
        return Ok(None);
    };
    let (file, result) = file.read().await?;
    keep(context, id, file);
    match result {
        Ok(data) => Ok(Some(data)),
        Err(error) => {
            warn(context, function, FileError::Io(error));
            Ok(None)
        }
    }
}

pub async fn write(
    context: &mut dyn Context,
    id: &PhpString,
    data: Vec<u8>,
    unchanged: bool,
    function: &str,
) -> Result<bool, NativeError> {
    let Some(file) = lock(context, id, function).await? else {
        return Ok(false);
    };
    let (file, result) = file.write(data, unchanged).await?;
    keep(context, id, file);
    match result {
        Ok(()) => Ok(true),
        Err(error) => {
            warn(context, function, FileError::Io(error));
            Ok(false)
        }
    }
}

pub async fn destroy(context: &mut dyn Context, id: &PhpString) -> Result<bool, NativeError> {
    let Some(files) = &mut state(context).files else {
        return Ok(false);
    };
    match files.locked.take() {
        Some((old, file)) if old == *id => file.destroy().await,
        Some((_, file)) => {
            let directory = files.directory.clone();
            blocking(move || drop(file)).await?;
            directory.destroy_existing(id).await
        }
        None => Ok(true),
    }
}

pub async fn collect(context: &mut dyn Context, lifetime: i64) -> Result<Option<i64>, NativeError> {
    let Some(directory) = state(context)
        .files
        .as_ref()
        .map(|files| files.directory.clone())
    else {
        return Ok(None);
    };
    directory.gc(lifetime).await
}

async fn lock(
    context: &mut dyn Context,
    id: &PhpString,
    function: &str,
) -> Result<Option<LockedFile>, NativeError> {
    let Some(files) = &mut state(context).files else {
        return Ok(None);
    };
    if let Some((old, file)) = files.locked.take()
        && old == *id
    {
        return Ok(Some(file));
    }
    let directory = files.directory.clone();
    match directory.lock(id).await? {
        Ok(file) => Ok(Some(file)),
        Err(error) => {
            warn(context, function, error);
            Ok(None)
        }
    }
}

fn keep(context: &mut dyn Context, id: &PhpString, file: LockedFile) {
    if let Some(files) = &mut state(context).files {
        files.locked = Some((id.clone(), file));
    }
}

fn warn(context: &mut dyn Context, function: &str, error: FileError) {
    let message = match error {
        FileError::Id => "Session ID is too long or contains illegal characters. Only the \
            A-Z, a-z, 0-9, \"-\", and \",\" characters are allowed"
            .into(),
        FileError::Path => {
            let limit = if cfg!(target_os = "macos") {
                1024
            } else {
                4096
            };
            format!(
                "Failed to create session data file path. Too short session ID, invalid \
                save_path or path length exceeds {limit} characters"
            )
        }
        FileError::Owner => "Session data file is not created by your uid".into(),
        FileError::Open(path, error) => {
            let errno = error.raw_os_error().unwrap_or_default();
            let text = error.to_string();
            let text = text.split(" (os error ").next().unwrap_or(&text);
            format!("open({}, O_RDWR) failed: {text} ({errno})", path.display())
        }
        FileError::Io(error) => format!("Session file I/O failed: {error}"),
    };
    warning(context, function, &message);
}
