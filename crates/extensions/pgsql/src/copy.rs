//! `pg_copy_from($db, 'table', $rows)` sends COPY data through the shared PostgreSQL client.

use phera_extension_api::traversal::{self, IteratorStep};
use phera_extension_api::value::{to_bool, to_php_string};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args};

use crate::{connection, escape};

pub fn copy<const FROM: bool>(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let call = if FROM { "pg_copy_from" } else { "pg_copy_to" };
        let state = connection::argument(c, args::value(&a, 0), call)?;
        let lease = connection::lease(&state).await?;
        let source = args::string(&a, 1).map_or_else(String::new, |s| s.to_string_lossy());
        let sep_arg = if FROM { 3 } else { 2 };
        let separator =
            args::string(&a, sep_arg).map_or_else(|| "\t".into(), |s| s.to_string_lossy());
        if separator.len() != 1 {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{} ($separator) must be one character",
                sep_arg + 1
            )));
        }
        let null =
            args::string(&a, sep_arg + 1).map_or_else(|| "\\N".into(), |s| s.to_string_lossy());
        let direction = if FROM { "FROM STDIN" } else { "TO STDOUT" };
        let command = format!(
            "COPY {} {direction} WITH (DELIMITER {}, NULL {})",
            escape::table_name(&source),
            escape::literal(&separator),
            escape::literal(&null)
        );
        let rows = if FROM {
            copy_rows(c, args::value(&a, 2)).await?
        } else {
            Vec::new()
        };
        let result = connection::run(&lease, move |pg| {
            Box::pin(async move {
                if FROM {
                    pg.native_copy_from(&command, rows)
                        .await
                        .map(|_| Vec::new())
                } else {
                    pg.native_copy_to(&command).await
                }
            })
        })
        .await;
        match result {
            Ok(_) if FROM => Ok(Value::Bool(true)),
            Ok(rows) => {
                let bytes: Vec<u8> = rows.into_iter().flatten().collect();
                let mut rows = PhpArray::new();
                for row in bytes.split_inclusive(|byte| *byte == b'\n') {
                    rows.push(Value::from(row.to_vec()))?;
                }
                Ok(Value::from(rows))
            }
            Err(e) => {
                state.borrow_mut().last_error.clone_from(&e.detail);
                c.warning(format!("{call}(): Copy command failed: {}", e.detail));
                Ok(Value::Bool(false))
            }
        }
    })
}

async fn copy_rows(c: &mut dyn Context, values: &Value) -> Result<Vec<Vec<u8>>, NativeError> {
    let mut rows = Vec::new();
    match &*values.deref() {
        Value::Array(values) => {
            for (_, value) in values.iter() {
                rows.push(copy_row(c, value)?);
            }
        }
        Value::Object(object) => {
            let mut cursor = c.traverse(object.clone()).await?;
            traversal::step(cursor.as_mut(), c, IteratorStep::Rewind).await?;
            while to_bool(&traversal::step(cursor.as_mut(), c, IteratorStep::Valid).await?) {
                let value = traversal::step(cursor.as_mut(), c, IteratorStep::Current).await?;
                rows.push(copy_row(c, &value)?);
                traversal::step(cursor.as_mut(), c, IteratorStep::Advance).await?;
            }
        }
        _ => return Err(NativeError::error("Invalid COPY row source")),
    }
    Ok(rows)
}

fn copy_row(c: &mut dyn Context, value: &Value) -> Result<Vec<u8>, NativeError> {
    let value = to_php_string(value, c.diagnostics())?;
    let mut row = value.as_bytes().to_vec();
    if row.last() != Some(&b'\n') {
        row.push(b'\n');
    }
    Ok(row)
}

pub fn line<const END: bool>(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let call = if END { "pg_end_copy" } else { "pg_put_line" };
        let explicit = args::object(&a, 0).is_some();
        let conn = if explicit {
            args::value(&a, 0)
        } else {
            &Value::Null
        };
        let handle = connection::argument(c, conn, call)?;
        let lease = connection::lease(&handle).await?;
        let row = args::bytes(&a, usize::from(explicit)).to_vec();
        let status = connection::run(&lease, move |driver| {
            Box::pin(async move {
                if END {
                    driver.end_copy().await
                } else {
                    driver.put_line(row).await
                }
            })
        })
        .await;
        match status {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => {
                handle.borrow_mut().last_error.clone_from(&error.detail);
                c.warning(format!("{call}(): {}", error.detail));
                Ok(Value::Bool(false))
            }
        }
    })
}
