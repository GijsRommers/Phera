//! `getmyuid()` and `get_current_user()` report the owner of the entry
//! script. An inline script has no file owner.

use std::os::unix::fs::MetadataExt;

use phera_extension_api::{Context, FunctionEntry, LocalFuture, NativeResult, Registry, Value};

pub(super) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::asynchronous("getmyuid", &[], uid),
        FunctionEntry::asynchronous("getmygid", &[], gid),
        FunctionEntry::asynchronous("getmyinode", &[], inode),
        FunctionEntry::asynchronous("get_current_user", &[], user),
    ] {
        registry.function(entry);
    }
}

async fn metadata(context: &dyn Context) -> Option<std::fs::Metadata> {
    tokio::fs::metadata(context.entry_script_path()?).await.ok()
}

fn uid(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        Ok(metadata(context)
            .await
            .map_or(Value::Bool(false), |file| Value::Int(i64::from(file.uid()))))
    })
}

fn gid(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        Ok(metadata(context)
            .await
            .map_or(Value::Bool(false), |file| Value::Int(i64::from(file.gid()))))
    })
}

fn inode(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        Ok(metadata(context)
            .await
            .map_or(Value::Bool(false), |file| Value::Int(file.ino() as i64)))
    })
}

fn user(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(file) = metadata(context).await else {
            return Ok(Value::from(""));
        };
        let uid = nix::unistd::Uid::from_raw(file.uid());
        let user = tokio::task::spawn_blocking(move || nix::unistd::User::from_uid(uid))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
        Ok(Value::from(user.map_or_else(String::new, |user| user.name)))
    })
}
