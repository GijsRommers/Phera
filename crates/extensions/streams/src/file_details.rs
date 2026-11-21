//! `linkinfo($path)`, `getlastmod()` and `ftok($path, 'p')` use local file metadata.

use std::{fs, io, os::unix::fs::MetadataExt, path::PathBuf};

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::errors::{check_path, reason};
use crate::handles::FILENAME;
use crate::request::{directory, path};

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "linkinfo",
        const { &[Param::new("path", ParamType::String)] },
        linkinfo,
    ));
    registry.function(FunctionEntry::sync("getlastmod", &[], getlastmod));
    registry.function(FunctionEntry::sync(
        "ftok",
        const { &[FILENAME, Param::new("project_id", ParamType::String)] },
        ftok,
    ));
    for (name, returns) in [
        ("linkinfo", "int|false"),
        ("getlastmod", "int|false"),
        ("ftok", "int"),
    ] {
        registry.describe_function(name, returns, Some("standard"));
    }
}

fn linkinfo(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let name = args::bytes(values, 0);
    check_path("linkinfo", "#1 ($path)", name)?;
    let result = local_file(context, name).and_then(fs::symlink_metadata);
    Ok(match result {
        Ok(metadata) => Value::Int(metadata.dev() as i64),
        Err(error) => {
            context.warning(format!("linkinfo(): {}", reason(&error)));
            Value::Int(-1)
        }
    })
}

fn getlastmod(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .entry_script_path()
        .and_then(|file| fs::metadata(file).ok())
        .map_or(Value::Bool(false), |metadata| Value::Int(metadata.mtime())))
}

fn ftok(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let name = args::bytes(values, 0);
    check_path("ftok", "#1 ($filename)", name)?;
    if name.is_empty() {
        return Err(NativeError::value_error(
            "ftok(): Argument #1 ($filename) must not be empty",
        ));
    }
    let [id] = args::bytes(values, 1) else {
        return Err(NativeError::value_error(
            "ftok(): Argument #2 ($project_id) must be a single character",
        ));
    };
    let result = local_file(context, name).and_then(fs::metadata);
    Ok(match result {
        Ok(metadata) => {
            let key = (u64::from(*id) << 24)
                | ((metadata.dev() & 0xff) << 16)
                | (metadata.ino() & 0xffff);
            Value::Int(i64::from(key as i32))
        }
        Err(error) => {
            context.warning(format!("ftok(): ftok() failed - {}", reason(&error)));
            Value::Int(-1)
        }
    })
}

fn local_file(context: &mut dyn Context, name: &[u8]) -> io::Result<PathBuf> {
    if name.is_empty() {
        return Err(rustix::io::Errno::NOENT.into());
    }
    directory(context).resolve(path(name))
}
