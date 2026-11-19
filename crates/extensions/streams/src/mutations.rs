//! `mkdir('cache/build', 0755, true)` creates directories with the request's umask.

use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry,
    Value, args,
};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, reason, warn_false, wrapper_warning};
use crate::handles::FILENAME;
use crate::io_policy::IoPolicy;
use crate::request::{bytes, directory, files, path};
use crate::wrapper::Target;

pub const DIRECTORY: Param = Param::new("directory", ParamType::String);
pub const PATH: Param = Param::new("path", ParamType::String);
pub const PERMISSIONS: Param = Param::new("permissions", ParamType::Int);

pub fn register(registry: &mut Registry) {
    const LINK: &[Param] = &[
        Param::new("target", ParamType::String),
        Param::new("link", ParamType::String),
    ];
    const MKDIR: &[Param] = &[
        DIRECTORY,
        PERMISSIONS.with_default(ConstantValue::Int(0o777)),
        Param::new("recursive", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        OPTIONAL_CONTEXT,
    ];
    const UMASK: &[Param] = &[Param::new("mask", ParamType::Int).optional().nullable()];
    for entry in [
        FunctionEntry::asynchronous("mkdir", MKDIR, mkdir),
        FunctionEntry::asynchronous("unlink", &[FILENAME, OPTIONAL_CONTEXT], unlink),
        FunctionEntry::asynchronous("rmdir", &[DIRECTORY, OPTIONAL_CONTEXT], rmdir),
        FunctionEntry::asynchronous("symlink", LINK, make_symlink),
        FunctionEntry::asynchronous("link", LINK, make_link),
        FunctionEntry::sync("readlink", &[PATH], readlink),
        FunctionEntry::sync("umask", UMASK, umask),
    ] {
        registry.function(entry);
    }
}

fn mkdir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        let permissions = args::int(&arguments, 1).unwrap_or(0o777) as u32;
        let recursive = args::bool(&arguments, 2).unwrap_or(false);
        check_path("mkdir", "#1 ($directory)", name)?;
        validate(args::value(&arguments, 3), "mkdir", 4)?;
        let mask = files(context).mask;
        let result = local(context, "mkdir", name).map(|name| (name, permissions & !mask));
        let result = match result {
            Ok((name, mode)) => {
                IoPolicy::Blocking
                    .run(move || create_directory(&name, mode, recursive))
                    .await?
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if matches!(Target::parse(name), Target::Data(_) | Target::Php(_)) {
                return Ok(Value::Bool(false));
            }
            return warn_false(context, format!("mkdir(): {}", reason(&error)));
        }
        Ok(Value::Bool(true))
    })
}

fn create_directory(name: &Path, mode: u32, recursive: bool) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.mode(mode);
    if recursive {
        let mut missing = Vec::new();
        let mut parent = name.parent();
        while let Some(path) = parent {
            match path.metadata() {
                Ok(_) => break,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    missing.push(path);
                    parent = path.parent();
                }
                Err(error) => return Err(error),
            }
        }
        for parent in missing.into_iter().rev() {
            match builder.create(parent) {
                Ok(()) => fs::set_permissions(parent, Permissions::from_mode(mode))?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && parent.is_dir() => (),
                Err(error) => return Err(error),
            }
        }
    }
    builder.create(name)?;
    fs::set_permissions(name, Permissions::from_mode(mode))
}

fn unlink(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(remove(
        context,
        arguments,
        "unlink",
        fs::remove_file::<PathBuf>,
    ))
}

fn rmdir(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(remove(
        context,
        arguments,
        "rmdir",
        fs::remove_dir::<PathBuf>,
    ))
}

async fn remove(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    operation: fn(PathBuf) -> io::Result<()>,
) -> NativeResult {
    let name = args::bytes(&arguments, 0);
    let parameter = if function == "unlink" {
        "#1 ($filename)"
    } else {
        "#1 ($directory)"
    };
    check_path(function, parameter, name)?;
    validate(args::value(&arguments, 1), function, 2)?;
    let result = match local(context, function, name) {
        Ok(name) => IoPolicy::Blocking.run(move || operation(name)).await?,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(Value::Bool(true)),
        Err(_) if matches!(Target::parse(name), Target::Data(_) | Target::Php(_)) => {
            if function == "unlink" {
                let wrapper = if matches!(Target::parse(name), Target::Data(_)) {
                    "RFC2397"
                } else {
                    "PHP"
                };
                return warn_false(
                    context,
                    format!("{function}(): {wrapper} does not allow unlinking"),
                );
            }
            Ok(Value::Bool(false))
        }
        Err(error) => warn_false(
            context,
            format!("{}: {}", path_call(function, name), reason(&error)),
        ),
    }
}

pub fn path_call(function: &str, name: &[u8]) -> String {
    format!("{function}({})", String::from_utf8_lossy(name))
}

pub fn local(context: &mut dyn Context, function: &str, name: &[u8]) -> io::Result<PathBuf> {
    if name.is_empty() {
        return Err(rustix::io::Errno::NOENT.into());
    }
    match Target::parse(name) {
        Target::File(name) => directory(context).resolve(path(name)),
        Target::Unavailable(scheme) => {
            wrapper_warning(context, function, scheme);
            directory(context).resolve(path(name))
        }
        Target::RemoteFile => {
            context.warning(format!(
                "{function}(): Remote host file access not supported, {}",
                String::from_utf8_lossy(name)
            ));
            Err(io::ErrorKind::Unsupported.into())
        }
        _ => Err(io::ErrorKind::Unsupported.into()),
    }
}

fn make_link(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(link(context, arguments, "link", Link::Hard))
}

fn make_symlink(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(link(context, arguments, "symlink", Link::Symbolic))
}

enum Link {
    Hard,
    Symbolic,
}

async fn link(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    link: Link,
) -> NativeResult {
    let name = args::bytes(&arguments, 0);
    let destination = args::bytes(&arguments, 1);
    check_path(function, "#1 ($target)", name)?;
    check_path(function, "#2 ($link)", destination)?;
    let result = match link {
        Link::Hard => local(context, function, name),
        // The relative path is stored verbatim and resolved relative to the link on lookup.
        Link::Symbolic => Ok(path(name).to_owned()),
    };
    let result = result.and_then(|name| {
        local(context, function, destination).map(|destination| (name, destination))
    });
    let result = match result {
        Ok((name, destination)) => {
            IoPolicy::Blocking
                .run(move || match link {
                    Link::Hard => hard_link(&name, &destination),
                    Link::Symbolic => symlink(name, link_destination(destination)?),
                })
                .await?
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => warn_false(context, format!("{function}(): {}", reason(&error))),
    }
}

fn hard_link(name: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    if destination.symlink_metadata().is_ok() {
        return Err(rustix::io::Errno::EXIST.into());
    }
    fs::hard_link(name, destination)
}

fn link_destination(mut destination: PathBuf) -> io::Result<PathBuf> {
    // PHP expands the destination first, including a final dangling link.
    for _ in 0..40 {
        match destination.read_link() {
            Ok(target) if target.is_absolute() => destination = target,
            Ok(target) => {
                destination.pop();
                destination.push(target);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(destination);
            }
            Err(error) => return Err(error),
        }
    }
    Err(rustix::io::Errno::LOOP.into())
}

fn readlink(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    check_path("readlink", "#1 ($path)", name)?;
    match read_link(context, name) {
        Ok(target) => Ok(Value::from(target)),
        Err(error) => warn_false(context, format!("readlink(): {}", reason(&error))),
    }
}

pub(crate) fn read_link(context: &mut dyn Context, name: &[u8]) -> io::Result<Vec<u8>> {
    if name.is_empty() {
        return Err(rustix::io::Errno::NOENT.into());
    }
    let resolved = directory(context).resolve(path(name))?;
    fs::read_link(resolved).map(|target| bytes(&target).to_vec())
}

fn umask(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let request = files(context);
    let old = request.mask;
    if let Some(mask) = args::int(arguments, 0) {
        request.mask = mask as u32 & 0o777;
    }
    Ok(Value::Int(i64::from(old)))
}
