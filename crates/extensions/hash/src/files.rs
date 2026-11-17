//! Hashing a local file, read asynchronously so that other requests run on
//! the thread while it waits for the disk:
//!
//! ```php
//! hash_file('sha256', '/etc/hosts');
//! md5_file('/missing');   // false, with "Failed to open stream" as a warning
//! hash_update_file($context, '/etc/hosts');
//! ```
//!
//! PHP's stream wrappers (`php://`, `http://`) are not supported yet.

use std::io::ErrorKind;
use std::path::PathBuf;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};
use tokio::io::AsyncReadExt;

use crate::algorithms::Algorithm;
use crate::digests::output;
use crate::given;
use crate::hmac::Hmac;
use crate::incremental::{State, hash_context, put_state, take_state};
use crate::options::{self, Options};

pub const FUNCTIONS: &[FunctionEntry] = &[
    HASH_FILE,
    HASH_HMAC_FILE,
    MD5_FILE,
    SHA1_FILE,
    HASH_UPDATE_FILE,
];

/// How much of the file is read at a time.
const CHUNK: usize = 8192;

/// Which argument holds the file name, as an error message names it.
struct FileArgument {
    function: &'static str,
    /// `Argument #2 ($filename)`
    argument: &'static str,
}

/// Feeds the file into `state`. `Ok(false)` when it cannot be opened or
/// read, after PHP's warning; an error for a name PHP rejects outright.
async fn read_into(
    context: &mut dyn Context,
    file: FileArgument,
    filename: &[u8],
    state: &mut State,
) -> Result<bool, NativeError> {
    let FileArgument { function, argument } = file;
    if filename.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): {argument} must not contain any null bytes"
        )));
    }
    if filename.is_empty() {
        return Err(NativeError::value_error("Path must not be empty"));
    }
    let mut file = match tokio::fs::File::open(path(filename)).await {
        Ok(file) => file,
        Err(error) => {
            let reason = match error.kind() {
                ErrorKind::NotFound => "No such file or directory".to_string(),
                ErrorKind::PermissionDenied => "Permission denied".to_string(),
                _ => error.to_string(),
            };
            context.warning(format!(
                "{function}({}): Failed to open stream: {reason}",
                String::from_utf8_lossy(filename)
            ));
            return Ok(false);
        }
    };
    let mut buffer = [0; CHUNK];
    loop {
        match file.read(&mut buffer).await {
            Ok(0) => return Ok(true),
            Ok(count) => state.update(&buffer[..count]),
            Err(error) => {
                context.warning(format!("{function}(): Read failed: {error}"));
                return Ok(false);
            }
        }
    }
}

/// A PHP file name is bytes; on Unix a path is too.
#[cfg(unix)]
fn path(filename: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(filename).into()
}

#[cfg(not(unix))]
fn path(filename: &[u8]) -> PathBuf {
    String::from_utf8_lossy(filename).into_owned().into()
}

/// The digest of a whole file, or `false` when it cannot be read.
async fn hash_whole_file(
    context: &mut dyn Context,
    file: FileArgument,
    filename: &[u8],
    mut state: State,
    binary: bool,
) -> NativeResult {
    if read_into(context, file, filename, &mut state).await? {
        Ok(output(state.finish(), binary))
    } else {
        Ok(Value::Bool(false))
    }
}

/// `hash_file(string $algo, string $filename, bool $binary = false, array $options = []): string|false`
const HASH_FILE: FunctionEntry = FunctionEntry::asynchronous(
    "hash_file",
    &[
        Param::new("algo", ParamType::String),
        Param::new("filename", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
        Param::new("options", ParamType::Array).optional(),
    ],
    hash_file,
);

fn hash_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm = options::algorithm(args::bytes(&arguments, 0), "hash_file")?;
        let options = Options::read(context, algorithm, arguments.get(3), "hash_file")?;
        let file = FileArgument {
            function: "hash_file",
            // PHP 8.5 names the wrong argument in this one message.
            argument: "Argument #1 ($algo)",
        };
        let state = State::new(algorithm, options, None);
        let binary = args::bool(&arguments, 2).unwrap_or(false);
        hash_whole_file(context, file, args::bytes(&arguments, 1), state, binary).await
    })
}

/// `hash_hmac_file(string $algo, string $filename, string $key, bool $binary = false): string|false`
const HASH_HMAC_FILE: FunctionEntry = FunctionEntry::asynchronous(
    "hash_hmac_file",
    &[
        Param::new("algo", ParamType::String),
        Param::new("filename", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    hash_hmac_file,
);

fn hash_hmac_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm =
            options::cryptographic_algorithm(args::bytes(&arguments, 0), "hash_hmac_file")?;
        let hmac = Hmac::new(algorithm, args::bytes(&arguments, 2));
        let file = FileArgument {
            function: "hash_hmac_file",
            argument: "Argument #2 ($filename)",
        };
        let state = State::new(algorithm, Options::default(), Some(hmac));
        let binary = args::bool(&arguments, 3).unwrap_or(false);
        hash_whole_file(context, file, args::bytes(&arguments, 1), state, binary).await
    })
}

/// `md5_file(string $filename, bool $binary = false): string|false`
const MD5_FILE: FunctionEntry = FunctionEntry::asynchronous(
    "md5_file",
    &[
        Param::new("filename", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    md5_file,
);

fn md5_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(file_of(context, arguments, "md5_file", Algorithm::Md5))
}

/// `sha1_file(string $filename, bool $binary = false): string|false`
const SHA1_FILE: FunctionEntry = FunctionEntry::asynchronous(
    "sha1_file",
    &[
        Param::new("filename", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    sha1_file,
);

fn sha1_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(file_of(context, arguments, "sha1_file", Algorithm::Sha1))
}

/// `md5_file()` and `sha1_file()`, which differ only in the algorithm.
async fn file_of(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &'static str,
    algorithm: Algorithm,
) -> NativeResult {
    let file = FileArgument {
        function,
        argument: "Argument #1 ($filename)",
    };
    let state = State::new(algorithm, Options::default(), None);
    let binary = args::bool(&arguments, 1).unwrap_or(false);
    hash_whole_file(context, file, args::bytes(&arguments, 0), state, binary).await
}

/// `hash_update_file(HashContext $context, string $filename, ?resource $stream_context = null): bool`
const HASH_UPDATE_FILE: FunctionEntry = FunctionEntry::asynchronous(
    "hash_update_file",
    &[
        Param::new("context", ParamType::Mixed),
        Param::new("filename", ParamType::String),
        Param::new("stream_context", ParamType::Mixed).optional(),
    ],
    hash_update_file,
);

fn hash_update_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = hash_context(&arguments, "hash_update_file")?;
        let stream_context = args::value(&arguments, 2);
        if !matches!(stream_context, Value::Null | Value::Resource(_)) {
            return Err(NativeError::type_error(format!(
                "hash_update_file(): Argument #3 ($stream_context) must be of type \
                 resource or null, {} given",
                given(stream_context)
            )));
        }
        // The state leaves the object while the file is read, because a
        // borrow of the object cannot be held across an await. Nothing else
        // in this request runs meanwhile, and the state goes back whatever
        // the outcome, so the context stays usable.
        let mut state = take_state(object, "hash_update_file")?;
        let file = FileArgument {
            function: "hash_update_file",
            argument: "Argument #2 ($filename)",
        };
        let read = read_into(context, file, args::bytes(&arguments, 1), &mut state).await;
        put_state(object, state);
        Ok(Value::Bool(read?))
    })
}
