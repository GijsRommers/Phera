//! `pg_copy_from($db, 'table', $rows)` sends COPY data through the shared PostgreSQL client.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args};

use crate::{connection, escape};

pub fn copy<const FROM: bool>(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let call = if FROM { "pg_copy_from" } else { "pg_copy_to" };
        let state = connection::argument(c, args::value(&a, 0), call)?;
        let lease = state.borrow().lease()?;
        let source = args::string(&a, 1).map_or_else(String::new, |s| s.to_string_lossy());
        let sep_arg = if FROM { 3 } else { 2 };
        let separator =
            args::string(&a, sep_arg).map_or_else(|| "\t".into(), |s| s.to_string_lossy());
        if separator.len() != 1 {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{} ($separator) must be a single character",
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
        let mut rows = Vec::new();
        if FROM {
            let Some(values) = args::array(&a, 2) else {
                return Err(NativeError::error(
                    "pg_copy_from(): Traversable rows are not supported",
                ));
            };
            for (_, row) in values.iter() {
                let mut row = phera_extension_api::value::to_php_string(row, c.diagnostics())?
                    .as_bytes()
                    .to_vec();
                if row.last() != Some(&b'\n') {
                    row.push(b'\n');
                }
                rows.push(row);
            }
        }
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
