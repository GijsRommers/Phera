//! Composer asks `file_exists($directory . '/Greeter.php')` before including a PSR-4 file.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "dirname",
        &[
            Param::new("path", ParamType::String),
            Param::new("levels", ParamType::Int).optional(),
        ],
        dirname,
    ),
    FunctionEntry::asynchronous(
        "file_exists",
        &[Param::new("filename", ParamType::String)],
        file_exists,
    ),
];

/// dirname is lexical. In particular it retains a written ".." component.
fn dirname(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let path =
        args::string(arguments, 0).ok_or_else(|| NativeError::error("Missing dirname path"))?;
    let levels = args::int(arguments, 1).unwrap_or(1);
    if levels < 1 {
        return Err(NativeError::value_error(
            "dirname(): Argument #2 ($levels) must be greater than or equal to 1",
        ));
    }
    let mut path = path.as_bytes();
    for _ in 0..levels {
        let parent = parent(path);
        if parent == path {
            break;
        }
        path = parent;
    }
    Ok(Value::from(phera_extension_api::PhpString::from(path)))
}

fn separator(byte: u8) -> bool {
    byte == b'/' || (cfg!(windows) && byte == b'\\')
}

fn parent(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return path;
    }
    let Some(end) = path.iter().rposition(|byte| !separator(*byte)) else {
        return &path[..1];
    };
    let Some(boundary) = path[..end].iter().rposition(|byte| separator(*byte)) else {
        return b".";
    };
    let prefix = &path[..boundary];
    prefix
        .iter()
        .rposition(|byte| !separator(*byte))
        .map_or(&path[..1], |last| &prefix[..=last])
}

fn file_exists(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let name = args::string(&arguments, 0)
        .map(|value| value.as_bytes().to_vec())
        .unwrap_or_default();
    Box::pin(async move {
        let exists = tokio::task::spawn_blocking(move || {
            #[cfg(unix)]
            let path = {
                use std::os::unix::ffi::OsStrExt;
                std::path::Path::new(std::ffi::OsStr::from_bytes(&name))
            };
            #[cfg(not(unix))]
            let display = String::from_utf8_lossy(&name);
            #[cfg(not(unix))]
            let path = std::path::Path::new(display.as_ref());
            std::fs::metadata(path).is_ok()
        })
        .await
        .map_err(|error| NativeError::error(format!("file_exists(): {error}")))?;
        Ok(Value::Bool(exists))
    })
}
