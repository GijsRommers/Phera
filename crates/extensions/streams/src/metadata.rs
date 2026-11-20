//! `is_file('link')` follows a symlink. `lstat('link')` describes the link itself.

use std::fs::{self, Metadata};
use std::os::unix::fs::{FileTypeExt, MetadataExt};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};
use rustix::fs::{Access, access, statvfs};

use crate::errors::{check_path, reason, warn_false, wrapper_warning};
use crate::handles::{FILENAME, STREAM};
use crate::request::{bytes, canonical, directory, path};
use crate::resource::{argument_resource, stream_arg, with_stream};
use crate::wrapper::Target;

pub fn register(registry: &mut Registry) {
    const DIRECTORY: &[Param] = &[Param::new("directory", ParamType::String)];
    const PATH: &[Param] = &[Param::new("path", ParamType::String)];
    const CACHE: &[Param] = &[
        Param::new("clear_realpath_cache", ParamType::Bool).optional(),
        FILENAME.optional(),
    ];
    for entry in [
        FunctionEntry::asynchronous("file_exists", &[FILENAME], file_exists),
        FunctionEntry::asynchronous("is_file", &[FILENAME], is_file),
        FunctionEntry::asynchronous("is_dir", &[FILENAME], is_dir),
        FunctionEntry::asynchronous("is_link", &[FILENAME], is_link),
        FunctionEntry::asynchronous("is_readable", &[FILENAME], is_readable),
        FunctionEntry::asynchronous("is_writable", &[FILENAME], is_writable),
        FunctionEntry::asynchronous("is_writeable", &[FILENAME], is_writeable),
        FunctionEntry::asynchronous("is_executable", &[FILENAME], is_executable),
        FunctionEntry::asynchronous("filesize", &[FILENAME], filesize),
        FunctionEntry::asynchronous("filemtime", &[FILENAME], filemtime),
        FunctionEntry::asynchronous("fileatime", &[FILENAME], fileatime),
        FunctionEntry::asynchronous("filectime", &[FILENAME], filectime),
        FunctionEntry::asynchronous("fileperms", &[FILENAME], fileperms),
        FunctionEntry::asynchronous("fileinode", &[FILENAME], fileinode),
        FunctionEntry::asynchronous("fileowner", &[FILENAME], fileowner),
        FunctionEntry::asynchronous("filegroup", &[FILENAME], filegroup),
        FunctionEntry::asynchronous("filetype", &[FILENAME], filetype),
        FunctionEntry::asynchronous("stat", &[FILENAME], stat),
        FunctionEntry::asynchronous("lstat", &[FILENAME], lstat),
        FunctionEntry::asynchronous("fstat", &[STREAM], fstat),
        FunctionEntry::sync("clearstatcache", CACHE, crate::user_stat::clear),
        FunctionEntry::sync("realpath", PATH, realpath),
        FunctionEntry::sync("disk_free_space", DIRECTORY, disk_free_space),
        FunctionEntry::sync("disk_total_space", DIRECTORY, disk_total_space),
    ] {
        registry.function(entry);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Query {
    Access(Access),
    IsFile,
    IsDirectory,
    IsLink,
    Type,
    Stat,
    LinkStat,
    Field(StatField),
}

impl Query {
    pub(crate) fn quiet(self) -> bool {
        matches!(
            self,
            Self::Access(_) | Self::IsFile | Self::IsDirectory | Self::IsLink
        )
    }

    pub(crate) fn follow(self) -> bool {
        !matches!(self, Self::IsLink | Self::LinkStat | Self::Type)
    }

    fn run<'a>(
        self,
        context: &'a mut dyn Context,
        args: Vec<Value>,
        function: &'static str,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            self.path_async(context, args::bytes(&args, 0), function, false)
                .await
        })
    }

    pub(crate) async fn path_async(
        self,
        context: &mut dyn Context,
        name: &[u8],
        function: &str,
        throws: bool,
    ) -> NativeResult {
        if !name.is_empty() && !name.contains(&0) {
            if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
                return crate::user_stat::url_stat(context, wrapper, name, function, self, throws)
                    .await;
            }
            if matches!(Target::parse(name), Target::File(_))
                && crate::user_wrappers::file_disabled(context)
            {
                return self.failed(context, name, function, throws);
            }
        }
        self.path(context, name, function, throws)
    }

    pub(crate) fn path(
        self,
        context: &mut dyn Context,
        name: &[u8],
        function: &str,
        throws: bool,
    ) -> NativeResult {
        if name.is_empty() {
            return Ok(Value::Bool(false));
        }
        // Unlike the path-typed APIs, PHP's stat family parses an ordinary string.
        if name.contains(&0) {
            if !self.quiet() {
                context.warning(format!("{function}(): Filename contains null byte"));
            }
            return Ok(Value::Bool(false));
        }
        let file = match Target::parse(name) {
            Target::File(file) => Some(file),
            Target::Unavailable(scheme) => {
                wrapper_warning(context, function, scheme);
                Some(name)
            }
            _ => None,
        };
        let resolved = file.and_then(|file| directory(context).resolve(path(file)).ok());
        if let Some(path) = resolved {
            if let Self::Access(mode) = self {
                return Ok(Value::Bool(access(path, mode).is_ok()));
            }
            let metadata = if self.follow() {
                fs::metadata(path)
            } else {
                fs::symlink_metadata(path)
            };
            if let Ok(metadata) = metadata {
                return self.value(&metadata);
            }
        }
        self.failed(context, name, function, throws)
    }

    pub(crate) fn failed(
        self,
        context: &mut dyn Context,
        name: &[u8],
        function: &str,
        throws: bool,
    ) -> NativeResult {
        if self.quiet() {
            return Ok(Value::Bool(false));
        }
        let operation = if self.follow() { "stat" } else { "Lstat" };
        let message = format!(
            "{function}(): {operation} failed for {}",
            String::from_utf8_lossy(name)
        );
        if throws {
            Err(NativeError::throw("RuntimeException", message))
        } else {
            warn_false(context, message)
        }
    }

    fn value(self, meta: &Metadata) -> NativeResult {
        Ok(match self {
            Self::IsFile => Value::Bool(meta.is_file()),
            Self::IsDirectory => Value::Bool(meta.is_dir()),
            Self::IsLink => Value::Bool(meta.is_symlink()),
            Self::Type => Value::from(file_type(meta.file_type())),
            Self::Field(field) => Value::Int(metadata_fields(meta)[field as usize]),
            Self::Stat | Self::LinkStat => return stat_array(metadata_fields(meta)),
            Self::Access(_) => Value::Bool(true),
        })
    }
}

fn file_type(kind: fs::FileType) -> &'static str {
    match kind {
        kind if kind.is_file() => "file",
        kind if kind.is_dir() => "dir",
        kind if kind.is_symlink() => "link",
        kind if kind.is_fifo() => "fifo",
        kind if kind.is_socket() => "socket",
        kind if kind.is_char_device() => "char",
        kind if kind.is_block_device() => "block",
        _ => "unknown",
    }
}

fn file_exists(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Access(Access::EXISTS).run(context, args, "file_exists")
}
fn is_readable(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Access(Access::READ_OK).run(context, args, "is_readable")
}
fn is_writable(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Access(Access::WRITE_OK).run(context, args, "is_writable")
}
fn is_writeable(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Access(Access::WRITE_OK).run(context, args, "is_writeable")
}
fn is_executable(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Access(Access::EXEC_OK).run(context, args, "is_executable")
}
fn is_file(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::IsFile.run(context, args, "is_file")
}
fn is_dir(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::IsDirectory.run(context, args, "is_dir")
}
fn is_link(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::IsLink.run(context, args, "is_link")
}
fn filetype(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Type.run(context, args, "filetype")
}
fn stat(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Stat.run(context, args, "stat")
}
fn lstat(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::LinkStat.run(context, args, "lstat")
}
fn filesize(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Size).run(context, args, "filesize")
}
fn filemtime(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Mtime).run(context, args, "filemtime")
}
fn fileatime(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Atime).run(context, args, "fileatime")
}
fn filectime(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Ctime).run(context, args, "filectime")
}
fn fileperms(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Mode).run(context, args, "fileperms")
}
fn fileinode(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Inode).run(context, args, "fileinode")
}
fn fileowner(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Uid).run(context, args, "fileowner")
}
fn filegroup(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Query::Field(StatField::Gid).run(context, args, "filegroup")
}

fn metadata_fields(meta: &Metadata) -> [i64; 13] {
    [
        meta.dev() as i64,
        meta.ino() as i64,
        i64::from(meta.mode()),
        meta.nlink() as i64,
        i64::from(meta.uid()),
        i64::from(meta.gid()),
        meta.rdev() as i64,
        meta.len() as i64,
        meta.atime(),
        meta.mtime(),
        meta.ctime(),
        meta.blksize() as i64,
        meta.blocks() as i64,
    ]
}

#[derive(Clone, Copy)]
pub(crate) enum StatField {
    Inode = 1,
    Mode = 2,
    Uid = 4,
    Gid = 5,
    Size = 7,
    Atime = 8,
    Mtime = 9,
    Ctime = 10,
}

pub(crate) const STAT_NAMES: [&str; 13] = [
    "dev", "ino", "mode", "nlink", "uid", "gid", "rdev", "size", "atime", "mtime", "ctime",
    "blksize", "blocks",
];

pub(crate) fn stat_array(fields: [i64; 13]) -> NativeResult {
    let mut result = PhpArray::new();
    for value in fields {
        result.push(Value::Int(value))?;
    }
    for (name, value) in STAT_NAMES.into_iter().zip(fields) {
        result.insert(ArrayKey::str(name), Value::Int(value));
    }
    Ok(Value::from(result))
}

fn fstat(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = argument_resource(args::value(&args, 0), "fstat", "#1 ($stream)")?;
        if let Some(file) = crate::user_file::resource_file(resource) {
            return match file.stat(context, "fstat").await? {
                Some(Value::Array(fields)) => {
                    stat_array(crate::user_stat::fields(context, &fields)?)
                }
                _ => Ok(Value::Bool(false)),
            };
        }
        fstat_native(resource, &args)
    })
}

fn fstat_native(resource: &phera_extension_api::value::Resource, args: &[Value]) -> NativeResult {
    if crate::directory::is_open(resource) {
        return Ok(Value::Bool(false));
    }
    let stream = stream_arg(args, "fstat")?;
    let fields = with_stream(stream, |stream| {
        if let Ok(meta) = stream.metadata() {
            Some(metadata_fields(&meta))
        } else {
            stream
                .memory_size()
                .map(|size| [12, 0, 0o100666, 1, 0, 0, -1, size as i64, 0, 0, 0, -1, -1])
        }
    })?;
    fields.map_or(Ok(Value::Bool(false)), stat_array)
}

fn realpath(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let name = args::bytes(args, 0);
    check_path("realpath", "#1 ($path)", name)?;
    Ok(real_path(context, name))
}

pub(crate) fn real_path(context: &mut dyn Context, name: &[u8]) -> Value {
    directory(context)
        .resolve(path(name))
        .and_then(|path| canonical(&path))
        .map_or(Value::Bool(false), |path| {
            Value::from(bytes(&path).to_vec())
        })
}

fn disk_free_space(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    disk_space(context, args, "disk_free_space", |stats| stats.f_bavail)
}
fn disk_total_space(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    disk_space(context, args, "disk_total_space", |stats| stats.f_blocks)
}
fn disk_space(
    context: &mut dyn Context,
    args: &[Value],
    function: &str,
    field: fn(&rustix::fs::StatVfs) -> u64,
) -> NativeResult {
    let name = args::bytes(args, 0);
    check_path(function, "#1 ($directory)", name)?;
    if name.is_empty() {
        return Ok(Value::Bool(false));
    }
    let result = directory(context)
        .resolve(path(name))
        .and_then(|name| statvfs(name).map_err(Into::into));
    match result {
        Ok(stats) => Ok(Value::Float(field(&stats) as f64 * stats.f_frsize as f64)),
        Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => Ok(Value::Bool(false)),
        Err(error) => warn_false(context, format!("{function}(): {}", reason(&error))),
    }
}
