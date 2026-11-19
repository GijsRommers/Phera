//! `posix_getpwuid(posix_geteuid())['name']` looks up the user through Unix NSS.

use std::os::unix::ffi::OsStrExt;

use nix::errno::Errno;
use nix::unistd::{Gid, Group, Uid, User};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};

pub(super) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous(
            "posix_getpwuid",
            const { &[Param::new("user_id", ParamType::Int)] },
            getpwuid,
        ),
        FunctionEntry::asynchronous(
            "posix_getpwnam",
            const { &[Param::new("username", ParamType::String)] },
            getpwnam,
        ),
        FunctionEntry::asynchronous(
            "posix_getgrgid",
            const { &[Param::new("group_id", ParamType::Int)] },
            getgrgid,
        ),
    ] {
        registry.function(entry);
    }
}

fn getpwuid(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let id = args::int(&arguments, 0).unwrap_or_default() as u32;
        lookup(context, move || User::from_uid(Uid::from_raw(id)), user).await
    })
}

fn getpwnam(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = crate::command::c_prefix(args::bytes(&arguments, 0));
        let Ok(name) = std::str::from_utf8(name) else {
            return Ok(Value::Bool(false));
        };
        let name = name.to_owned();
        lookup(context, move || User::from_name(&name), user).await
    })
}

fn getgrgid(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let id = args::int(&arguments, 0).unwrap_or_default() as u32;
        lookup(context, move || Group::from_gid(Gid::from_raw(id)), group).await
    })
}

async fn lookup<T: Send + 'static>(
    context: &mut dyn Context,
    query: impl FnOnce() -> Result<Option<T>, Errno> + Send + 'static,
    convert: fn(T) -> NativeResult,
) -> NativeResult {
    match tokio::task::spawn_blocking(query)
        .await
        .map_err(|error| NativeError::error(error.to_string()))?
    {
        Ok(Some(account)) => convert(account),
        Ok(None) => Ok(Value::Bool(false)),
        Err(error) => Ok(super::fail(context, error)),
    }
}

fn user(user: User) -> NativeResult {
    Ok(array([
        ("name", Value::from(user.name)),
        ("passwd", Value::from(user.passwd.as_bytes())),
        ("uid", Value::Int(i64::from(user.uid.as_raw()))),
        ("gid", Value::Int(i64::from(user.gid.as_raw()))),
        ("gecos", Value::from(user.gecos.as_bytes())),
        ("dir", Value::from(user.dir.as_os_str().as_bytes())),
        ("shell", Value::from(user.shell.as_os_str().as_bytes())),
    ]))
}

fn group(group: Group) -> NativeResult {
    let mut members = PhpArray::new();
    for member in group.mem {
        members.push(Value::from(member))?;
    }
    Ok(array([
        ("name", Value::from(group.name)),
        ("passwd", Value::from(group.passwd.as_bytes())),
        ("members", Value::from(members)),
        ("gid", Value::Int(i64::from(group.gid.as_raw()))),
    ]))
}

pub(super) fn array<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let mut array = PhpArray::new();
    for (name, value) in fields {
        array.insert(ArrayKey::str(name), value);
    }
    Value::from(array)
}
