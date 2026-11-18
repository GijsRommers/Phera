//! `touch('report', 1700000000)` sets both timestamps to the given second.

use std::fs::{self, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};
use rustix::fs::{AtFlags, CWD, Timestamps, UTIME_NOW, chownat, utimensat};
use rustix::process::{Gid, Uid};

use crate::errors::{check_path, reason, warn_false};
use crate::handles::FILENAME;
use crate::io_policy::IoPolicy;
use crate::mode::Mode;
use crate::mutations::{PERMISSIONS, local};
use crate::request::files;

pub fn register(registry: &mut Registry) {
    const USER: &[Param] = &[FILENAME, Param::new("user", ParamType::IntOrString)];
    const GROUP: &[Param] = &[FILENAME, Param::new("group", ParamType::IntOrString)];
    const TOUCH: &[Param] = &[
        FILENAME,
        Param::new("mtime", ParamType::Int).optional().nullable(),
        Param::new("atime", ParamType::Int).optional().nullable(),
    ];
    for entry in [
        FunctionEntry::asynchronous("chmod", &[FILENAME, PERMISSIONS], chmod),
        FunctionEntry::asynchronous("chown", USER, chown),
        FunctionEntry::asynchronous("lchown", USER, lchown),
        FunctionEntry::asynchronous("chgrp", GROUP, chgrp),
        FunctionEntry::asynchronous("lchgrp", GROUP, lchgrp),
        FunctionEntry::asynchronous("touch", TOUCH, touch),
    ] {
        registry.function(entry);
    }
}

fn chmod(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        check_path("chmod", "#1 ($filename)", name)?;
        let mode = args::int(&arguments, 1).unwrap_or(0) as u32;
        let resolved = local(context, "chmod", name);
        let result = match resolved {
            Ok(path) => {
                IoPolicy::Blocking
                    .run(move || fs::set_permissions(path, Permissions::from_mode(mode)))
                    .await?
            }
            Err(error) => Err(error),
        };
        result_value(context, "chmod", result)
    })
}

fn chown(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(change_owner(
        context,
        arguments,
        "chown",
        Owner::User,
        AtFlags::empty(),
    ))
}
fn lchown(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(change_owner(
        context,
        arguments,
        "lchown",
        Owner::User,
        AtFlags::SYMLINK_NOFOLLOW,
    ))
}
fn chgrp(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(change_owner(
        context,
        arguments,
        "chgrp",
        Owner::Group,
        AtFlags::empty(),
    ))
}
fn lchgrp(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(change_owner(
        context,
        arguments,
        "lchgrp",
        Owner::Group,
        AtFlags::SYMLINK_NOFOLLOW,
    ))
}

#[derive(Clone, Copy)]
enum Owner {
    User,
    Group,
}

impl Owner {
    fn id(self, name: &str) -> Option<u32> {
        match self {
            Self::User => nix::unistd::User::from_name(name)
                .ok()
                .flatten()
                .map(|user| user.uid.as_raw()),
            Self::Group => nix::unistd::Group::from_name(name)
                .ok()
                .flatten()
                .map(|group| group.gid.as_raw()),
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::User => "uid",
            Self::Group => "gid",
        }
    }
}

async fn change_owner(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    owner: Owner,
    flags: AtFlags,
) -> NativeResult {
    let name = args::bytes(&arguments, 0);
    check_path(function, "#1 ($filename)", name)?;
    let id = if let Some(id) = args::int(&arguments, 1) {
        id as u32
    } else {
        let username = String::from_utf8_lossy(args::bytes(&arguments, 1)).into_owned();
        let lookup = username.clone();
        let Some(id) = IoPolicy::Blocking.run(move || owner.id(&lookup)).await? else {
            return warn_false(
                context,
                format!(
                    "{function}(): Unable to find {} for {username}",
                    owner.kind()
                ),
            );
        };
        id
    };
    let path = local(context, function, name);
    let result = match path {
        Ok(path) => {
            IoPolicy::Blocking
                .run(move || {
                    let (user, group) = if id == u32::MAX {
                        (None, None)
                    } else {
                        match owner {
                            Owner::User => (Some(Uid::from_raw(id)), None),
                            Owner::Group => (None, Some(Gid::from_raw(id))),
                        }
                    };
                    chownat(CWD, path, user, group, flags).map_err(Into::into)
                })
                .await?
        }
        Err(error) => Err(error),
    };
    result_value(context, function, result)
}

fn touch(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        check_path("touch", "#1 ($filename)", name)?;
        let mtime = args::int(&arguments, 1);
        let atime = args::int(&arguments, 2);
        if mtime.is_none() && atime.is_some() {
            return Err(NativeError::value_error(concat!(
                "touch(): Argument #2 ($mtime) cannot be null ",
                "when argument #3 ($atime) is an integer"
            )));
        }
        let atime = atime.or(mtime);
        let mask = files(context).mask;
        let result = local(context, "touch", name);
        let result = match result {
            Ok(path) => {
                IoPolicy::Blocking
                    .run(move || touch_path(&path, mask, mtime, atime))
                    .await?
            }
            Err(error) => Err((error, "Unable to create file")),
        };
        match result {
            Ok(()) => Ok(Value::Bool(true)),
            Err((error, task)) => warn_false(
                context,
                format!(
                    "touch(): {task} {} because {}",
                    String::from_utf8_lossy(name),
                    reason(&error)
                ),
            ),
        }
    })
}

fn touch_path(
    path: &Path,
    mask: u32,
    mtime: Option<i64>,
    atime: Option<i64>,
) -> Result<(), (io::Error, &'static str)> {
    if !path.exists() {
        let mode = Mode::parse(b"c")
            .ok_or_else(|| (io::ErrorKind::InvalidInput.into(), "Unable to create file"))?;
        mode.open(path, mask)
            .map_err(|error| (error, "Unable to create file"))?;
    }
    let stamp = |time: Option<i64>| rustix::time::Timespec {
        tv_sec: time.unwrap_or(0),
        tv_nsec: if time.is_some() { 0 } else { UTIME_NOW },
    };
    let times = Timestamps {
        last_access: stamp(atime),
        last_modification: stamp(mtime),
    };
    utimensat(CWD, path, &times, AtFlags::empty())
        .map_err(|error| (error.into(), "Unable to set time on file"))
}

fn result_value(context: &mut dyn Context, function: &str, result: io::Result<()>) -> NativeResult {
    match result {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => warn_false(context, format!("{function}(): {}", reason(&error))),
    }
}
