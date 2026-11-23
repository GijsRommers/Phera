//! `pg_lo_export($db, $oid, $path)` writes to the PHP client's filesystem.

use std::path::{Path, PathBuf};

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use crate::{connection, lob};

pub fn transfer<const IMPORT: bool, const ALIAS: bool>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (canonical, alias) = if IMPORT {
            ("pg_lo_import", "pg_loimport")
        } else {
            ("pg_lo_export", "pg_loexport")
        };
        let name = if ALIAS {
            context.deprecated(format!(
                "Function {alias}() is deprecated since 8.0, use {canonical}() instead"
            ));
            alias
        } else {
            canonical
        };
        let explicit = args::object(&arguments, 0)
            .is_some_and(|object| object.class.is_instance_of("PgSql\\Connection"));
        let start = usize::from(explicit);
        if arguments.len() < start + if IMPORT { 1 } else { 2 } {
            let received = arguments.len();
            if IMPORT {
                return Err(NativeError::argument_count_error(format!(
                    "{name}() expects at least 2 arguments, {received} given"
                )));
            }
            return Err(NativeError::argument_count_error(format!(
                "{name}() Requires 2 or 3 arguments, {received} given"
            )));
        }
        let filename_at = start + usize::from(!IMPORT);
        let value = args::value(&arguments, filename_at);
        let path = phera_extension_api::value::to_php_string(value, context.diagnostics())?;
        if path.as_bytes().contains(&0) {
            let detail = if IMPORT {
                format!("Wrong parameter count for {name}()")
            } else {
                format!(
                    "{name}() Requires 2 or 3 arguments, {} given",
                    arguments.len()
                )
            };
            return Err(NativeError::argument_count_error(detail));
        }
        let state = connection::argument(
            context,
            if explicit {
                &arguments[0]
            } else {
                &Value::Null
            },
            name,
        )?;
        let path = native_path(path.as_bytes());
        if !permitted(context, name, &path).await {
            return Ok(Value::Bool(false));
        }
        let oid_at = start + usize::from(IMPORT);
        let oid = if IMPORT && arguments.len() == oid_at {
            0
        } else {
            lob::oid(args::value(&arguments, oid_at), IMPORT)?
        };
        let lease = crate::connection::lease(&state).await?;
        let result = connection::run(&lease, move |driver| {
            Box::pin(async move {
                if IMPORT {
                    driver
                        .import_lob(path, oid)
                        .await
                        .map(|oid| Some(i64::from(oid)))
                } else {
                    driver.export_lob(path, oid).await.map(|()| None)
                }
            })
        })
        .await;
        match result {
            Ok(oid) => Ok(oid.map_or(Value::Bool(true), Value::Int)),
            Err(error) => {
                state.borrow_mut().last_error = error.detail;
                Ok(Value::Bool(false))
            }
        }
    })
}

fn native_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

async fn permitted(context: &mut dyn Context, name: &str, path: &Path) -> bool {
    let Some(basedir) = context
        .ini_get(b"open_basedir")
        .filter(|value| !value.as_bytes().is_empty())
    else {
        return true;
    };
    let resolved = match tokio::fs::canonicalize(path).await {
        Ok(existing) => Some(existing),
        Err(_) if tokio::fs::symlink_metadata(path).await.is_err() => {
            let parent = path
                .parent()
                .filter(|dir| !dir.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            match (tokio::fs::canonicalize(parent).await, path.file_name()) {
                (Ok(parent), Some(filename)) => Some(parent.join(filename)),
                _ => None,
            }
        }
        Err(_) => None,
    };
    if let Some(resolved) = resolved {
        for root in std::env::split_paths(native_path(basedir.as_bytes()).as_os_str()) {
            if let Ok(root) = tokio::fs::canonicalize(root).await
                && resolved.starts_with(root)
            {
                return true;
            }
        }
    }
    context.warning(format!(
        "{name}(): open_basedir restriction in effect. File({}) is not within the allowed path(s): ({})",
        path.display(),
        basedir.to_string_lossy()
    ));
    false
}
