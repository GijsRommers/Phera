//! `hash_file('sha256', $path)` reads files asynchronously on Tokio.
use crate::{
    algorithms::Algorithm,
    functions,
    incremental::{self, State},
    options,
};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};
use tokio::io::AsyncReadExt;

async fn read(
    context: &mut dyn Context,
    function: &str,
    filename: &[u8],
    argument: usize,
    state: &mut State,
) -> Result<bool, NativeError> {
    if filename.contains(&0) {
        // PHP 8.5.12's hash_file diagnostic names the algorithm parameter here.
        let (argument, parameter) = if function == "hash_file" {
            (1, "algo")
        } else {
            (argument, "filename")
        };
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{argument} (${parameter}) must not contain any null bytes"
        )));
    }
    if filename.is_empty() {
        return Err(NativeError::value_error("Path must not be empty"));
    }
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::Path::new(std::ffi::OsStr::from_bytes(filename)).to_path_buf()
    };
    #[cfg(not(unix))]
    let path = std::path::PathBuf::from(String::from_utf8_lossy(filename).into_owned());
    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) => {
            let reason = match error.kind() {
                std::io::ErrorKind::NotFound => "No such file or directory".into(),
                std::io::ErrorKind::PermissionDenied => "Permission denied".into(),
                _ => error.to_string(),
            };
            context.warning(format!(
                "{function}({}): Failed to open stream: {reason}",
                String::from_utf8_lossy(filename)
            ));
            return Ok(false);
        }
    };
    let mut buffer = [0u8; 8192];
    loop {
        match file.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => state.update(&buffer[..count]),
            Err(error) => {
                context.warning(format!("{function}(): Read failed: {error}"));
                return Ok(false);
            }
        }
    }
    Ok(true)
}
pub fn hash_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut algorithm = options::algorithm(&arguments, "hash_file", false)?;
        options::apply(context, &mut algorithm, arguments.get(3), "hash_file")?;
        let mut state = State::new(algorithm, None);
        if !read(
            context,
            "hash_file",
            args::bytes(&arguments, 1),
            2,
            &mut state,
        )
        .await?
        {
            return Ok(Value::Bool(false));
        }
        Ok(functions::output(
            state.finish(),
            args::bool(&arguments, 2).unwrap_or(false),
        ))
    })
}
pub fn hmac_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm = options::algorithm(&arguments, "hash_hmac_file", true)?;
        let mut state = State::new(algorithm, Some(args::bytes(&arguments, 2).to_vec()));
        if !read(
            context,
            "hash_hmac_file",
            args::bytes(&arguments, 1),
            2,
            &mut state,
        )
        .await?
        {
            return Ok(Value::Bool(false));
        }
        Ok(functions::output(
            state.finish(),
            args::bool(&arguments, 3).unwrap_or(false),
        ))
    })
}
pub fn md5_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    standalone(context, arguments, "md5")
}
pub fn sha1_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    standalone(context, arguments, "sha1")
}
fn standalone<'a>(
    context: &'a mut dyn Context,
    arguments: Vec<Value>,
    name: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let mut state = State::new(Algorithm::find(name.as_bytes()).expect("known hash"), None);
        if !read(
            context,
            &format!("{name}_file"),
            args::bytes(&arguments, 0),
            1,
            &mut state,
        )
        .await?
        {
            return Ok(Value::Bool(false));
        }
        Ok(functions::output(
            state.finish(),
            args::bool(&arguments, 1).unwrap_or(false),
        ))
    })
}
pub fn update_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = incremental::object(&arguments, "hash_update_file")?;
        let mut state = {
            let native = object.native.borrow();
            native
                .as_ref()
                .and_then(|s| s.downcast_ref::<State>())
                .cloned()
                .ok_or_else(|| incremental::invalid("hash_update_file"))?
        };
        let success = read(
            context,
            "hash_update_file",
            args::bytes(&arguments, 1),
            2,
            &mut state,
        )
        .await?;
        *object.native.borrow_mut() = Some(Box::new(state));
        Ok(Value::Bool(success))
    })
}
