//! `file_get_contents('data:text/plain,Hello%20there')` reads bytes from a URL.

use std::fs;
use std::io::{self, Cursor};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::Arc;

use phera_extension_api::{Context, NativeError, Value};
use rustix::io::fcntl_dupfd_cloexec;

use crate::errors::{nonempty_path, open_failed, reason, wrapper_warning};
use crate::io_policy::IoPolicy;
use crate::mode::Mode;
use crate::request::{directory, files, path, resource, search};
use crate::stream::{Backing, Stream, Wrapper};

const TEMP_DEFAULT: u64 = 2 * 1024 * 1024;

pub enum Target<'a> {
    File(&'a [u8]),
    RemoteFile,
    Php(&'a [u8]),
    Data(&'a [u8]),
    Unavailable(&'a [u8]),
}

impl<'a> Target<'a> {
    pub fn parse(name: &'a [u8]) -> Self {
        if let Some(rest) = name.strip_prefix(b"data:") {
            return Self::Data(rest.strip_prefix(b"//").unwrap_or(rest));
        }
        let prefix = name
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(byte))
            .count();
        if prefix <= 1 || !name[prefix..].starts_with(b"://") {
            return Self::File(name);
        }
        let protocol = &name[..prefix];
        let rest = &name[prefix + 3..];
        if protocol.eq_ignore_ascii_case(b"file") {
            if rest.starts_with(b"/") {
                Self::File(rest)
            } else if rest
                .get(..10)
                .is_some_and(|host| host.eq_ignore_ascii_case(b"localhost/"))
            {
                Self::File(&rest[9..])
            } else {
                Self::RemoteFile
            }
        } else if protocol.eq_ignore_ascii_case(b"php") {
            Self::Php(rest)
        } else {
            Self::Unavailable(protocol)
        }
    }
}

struct OpenFailure {
    detail: String,
    resolved: Option<PathBuf>,
}

impl OpenFailure {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            resolved: None,
        }
    }
}

pub async fn open(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
    include_path: bool,
) -> Result<Option<Stream>, NativeError> {
    nonempty_path(name)?;
    // The PHP mode parser takes a C string and keeps at most 15 bytes in metadata.
    let mode = mode.split(|byte| *byte == 0).next().unwrap_or_default();
    let (result, warning) = match Target::parse(name) {
        Target::File(filename) => (
            open_file(context, filename, name, mode, include_path).await?,
            None,
        ),
        Target::RemoteFile => (
            Err(OpenFailure::new("no suitable wrapper could be found")),
            Some(format!(
                "{function}(): Remote host file access not supported, {}",
                String::from_utf8_lossy(name)
            )),
        ),
        Target::Php(target) => {
            match open_php(target, mode, name, files(context).input.clone()).await? {
                Some(result) => (
                    result.map_err(|error| OpenFailure::new(reason(&error))),
                    None,
                ),
                None => (
                    Err(OpenFailure::new("operation failed")),
                    Some(format!("{function}(): Invalid php:// URL specified")),
                ),
            }
        }
        Target::Data(data) => (
            crate::data_uri::open(data, mode, name).map_err(OpenFailure::new),
            None,
        ),
        Target::Unavailable(scheme) => {
            if include_path {
                wrapper_warning(context, function, scheme);
            }
            wrapper_warning(context, function, scheme);
            (open_file(context, name, name, mode, false).await?, None)
        }
    };
    if let Some(warning) = warning {
        context.warning(warning);
    }
    match result {
        Ok(stream) => Ok(Some(stream)),
        Err(failure) => {
            let uri = failure
                .resolved
                .as_deref()
                .map_or(name, crate::request::bytes);
            open_failed(context, function, uri, &failure.detail);
            Ok(None)
        }
    }
}

async fn open_file(
    context: &mut dyn Context,
    filename: &[u8],
    uri: &[u8],
    mode: &[u8],
    include_path: bool,
) -> Result<Result<Stream, OpenFailure>, NativeError> {
    let Some(parsed) = Mode::parse(mode) else {
        return Ok(Err(OpenFailure::new(format!(
            "`{}' is not a valid mode for fopen",
            String::from_utf8_lossy(mode)
        ))));
    };
    let include = include_path.then(|| (context.include_path(), context.script_path()));
    let mask = files(context).mask;
    let directory = directory(context);
    let found = include
        .and_then(|(include, script)| search(directory, filename, &include, script.as_deref()));
    let found_by_include = found.is_some();
    let filename = match found.map_or_else(|| directory.resolve(path(filename)), Ok) {
        Ok(filename) => filename,
        Err(error) => return Ok(Err(OpenFailure::new(reason(&error)))),
    };
    let mode = mode[..mode.len().min(15)].to_vec();
    let uri = uri.to_vec();
    let resolved = found_by_include.then(|| filename.clone());
    // Opening a FIFO can wait for the other endpoint even before the first read.
    let policy = match fs::metadata(&filename) {
        Ok(metadata) if !metadata.is_file() && !metadata.is_dir() => IoPolicy::Blocking,
        _ => IoPolicy::Inline,
    };
    policy
        .run(move || {
            parsed
                .open(&filename, mask)
                .map(|file| {
                    let mut stream = Stream::file(file, Wrapper::File, &mode, &uri);
                    stream.blocked = parsed.blocked();
                    stream
                })
                .map_err(|error| {
                    let detail = if error.kind() == io::ErrorKind::NotADirectory {
                        reason(&rustix::io::Errno::NOENT.into())
                    } else {
                        reason(&error)
                    };
                    OpenFailure { detail, resolved }
                })
        })
        .await
}

async fn open_php(
    target: &[u8],
    mode: &[u8],
    uri: &[u8],
    input: Arc<[u8]>,
) -> Result<Option<io::Result<Stream>>, NativeError> {
    let target = target.to_ascii_lowercase();
    let stream = match target.as_slice() {
        b"memory" => buffer(Wrapper::Memory, mode, uri),
        b"output" => Stream::new(Backing::Output, Wrapper::Output, b"wb", uri),
        b"input" => Stream::new(
            Backing::Input(Cursor::new(input)),
            Wrapper::Input,
            b"rb",
            uri,
        ),
        name if name.starts_with(b"temp") => {
            let mut stream = buffer(Wrapper::Temp, mode, uri);
            let limit = name
                .strip_prefix(b"temp/maxmemory:")
                .and_then(|number| std::str::from_utf8(number).ok())
                .and_then(|number| number.parse().ok())
                .unwrap_or(TEMP_DEFAULT);
            stream.spill_limit = Some(limit);
            stream
        }
        b"stdin" | b"stdout" | b"stderr" => {
            let mode = mode[..mode.len().min(15)].to_vec();
            let uri = uri.to_vec();
            return IoPolicy::Blocking
                .run(move || Some(standard(&target, &mode, &uri)))
                .await;
        }
        _ => return Ok(None),
    };
    Ok(Some(Ok(stream)))
}

fn buffer(kind: Wrapper, mode: &[u8], uri: &[u8]) -> Stream {
    let effective: &[u8] = if mode.contains(&b'a') {
        b"a+b"
    } else if mode.contains(&b'w') || mode.contains(&b'+') {
        b"w+b"
    } else {
        b"rb"
    };
    Stream::new(
        Backing::Memory(Cursor::new(Vec::new())),
        kind,
        effective,
        uri,
    )
}

fn standard(target: &[u8], mode: &[u8], uri: &[u8]) -> io::Result<Stream> {
    let descriptor = match target {
        b"stdin" => fcntl_dupfd_cloexec(io::stdin().as_fd(), 0)?,
        b"stdout" => fcntl_dupfd_cloexec(io::stdout().as_fd(), 0)?,
        _ => fcntl_dupfd_cloexec(io::stderr().as_fd(), 0)?,
    };
    Ok(Stream::file(
        descriptor.into(),
        Wrapper::Standard,
        mode,
        uri,
    ))
}

/// The three CLI-only constants own independent duplicates of the process descriptors.
pub fn standard_constants() -> impl Iterator<Item = (&'static str, Value)> {
    [
        (
            "STDIN",
            1,
            b"stdin".as_slice(),
            b"rb".as_slice(),
            b"php://stdin".as_slice(),
        ),
        ("STDOUT", 2, b"stdout", b"wb", b"php://stdout"),
        ("STDERR", 3, b"stderr", b"wb", b"php://stderr"),
    ]
    .into_iter()
    .filter_map(|(constant, id, target, mode, uri)| {
        standard(target, mode, uri)
            .ok()
            .map(|stream| (constant, resource(id, "stream", stream)))
    })
}

pub fn resolve_file(context: &mut dyn Context, name: &[u8]) -> io::Result<std::path::PathBuf> {
    match Target::parse(name) {
        Target::File(bytes) => directory(context).resolve(path(bytes)),
        _ => Err(io::ErrorKind::Unsupported.into()),
    }
}
