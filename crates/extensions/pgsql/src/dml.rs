//! `pg_insert($db, $table, $fields, PGSQL_DML_STRING)` returns generated SQL.

mod conversion;
mod metadata;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args,
    value::to_php_string,
};

use crate::{connection, escape, result};
use conversion::Field;

pub const META: usize = 0;
pub const CONVERT: usize = 1;
pub const INSERT: usize = 2;
pub const UPDATE: usize = 3;
pub const DELETE: usize = 4;
pub const SELECT: usize = 5;

pub fn execute<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let names = [
            "pg_meta_data",
            "pg_convert",
            "pg_insert",
            "pg_update",
            "pg_delete",
            "pg_select",
        ];
        let call = names.get(OP).copied().unwrap_or_default();
        let handle = connection::argument(c, args::value(&a, 0), call)?;
        let lease = handle.borrow().lease()?;
        let table = args::string(&a, 1).map_or_else(String::new, |value| value.to_string_lossy());
        let flags = args::int(&a, if OP == UPDATE { 4 } else { 3 }).unwrap_or(if OP == CONVERT {
            0
        } else {
            512
        });
        let columns = if OP == META || flags & (256 | 4096) == 0 {
            let sql = metadata::query(&table);
            let columns = connection::run(&lease, move |pg| {
                Box::pin(async move { pg.native_query(&sql).await })
            })
            .await
            .ok()
            .and_then(|mut rows| rows.pop())
            .map(metadata::columns)
            .unwrap_or_default();
            if columns.is_empty() {
                c.warning(format!("{call}(): Table '{table}' doesn't exists"));
                return Ok(Value::Bool(false));
            }
            columns
        } else {
            Vec::new()
        };
        if OP == META {
            return Ok(metadata::to_array(
                columns,
                args::bool(&a, 2).unwrap_or_default(),
            ));
        }
        let Some(fields) =
            conversion::convert(c, args::array(&a, 2).map(Rc::as_ref), &columns, flags, call)?
        else {
            return Ok(Value::Bool(false));
        };
        if OP == CONVERT {
            let mut converted = PhpArray::new();
            for Field { name, value } in fields {
                converted.insert(ArrayKey::str(name), value);
            }
            return Ok(Value::from(converted));
        }
        let conditions = if OP == UPDATE {
            let Some(conditions) =
                conversion::convert(c, args::array(&a, 3).map(Rc::as_ref), &columns, flags, call)?
            else {
                return Ok(Value::Bool(false));
            };
            conditions
        } else {
            Vec::new()
        };
        let table = escape::table_name(&table);
        let sql = match OP {
            INSERT => {
                if fields.is_empty() {
                    format!("INSERT INTO {table} DEFAULT VALUES;")
                } else {
                    let keys = fields
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    let mut values = Vec::new();
                    for field in fields {
                        values
                            .push(to_php_string(&field.value, c.diagnostics())?.to_string_lossy());
                    }
                    format!(
                        "INSERT INTO {table} ({keys}) VALUES ({});",
                        values.join(",")
                    )
                }
            }
            UPDATE => {
                let assignments = joined(c, &fields, ",", false)?;
                let conditions = joined(c, &conditions, " AND ", true)?;
                let predicate = if conditions.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {conditions}")
                };
                format!("UPDATE {table} SET {assignments}{predicate};")
            }
            _ => {
                let conditions = joined(c, &fields, " AND ", true)?;
                let predicate = if conditions.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {conditions}")
                };
                let verb = if OP == DELETE { "DELETE" } else { "SELECT *" };
                format!("{verb} FROM {table}{predicate};")
            }
        };
        if flags & 2048 != 0 {
            return Ok(Value::from(sql));
        }
        if flags & 1024 != 0 {
            let pending = connection::start(&lease, move |pg| {
                Box::pin(async move { pg.native_query(&sql).await })
            })
            .map_err(|error| NativeError::error(error.detail))?;
            handle.borrow_mut().busy = Some(pending);
            return Ok(Value::Bool(true));
        }
        let output = connection::run(&lease, move |pg| {
            Box::pin(async move { pg.native_query(&sql).await })
        })
        .await;
        match output {
            Ok(mut rows) => {
                let Some(rows) = rows.pop() else {
                    return Ok(Value::Bool(false));
                };
                if let Some(error) = rows.error {
                    handle.borrow_mut().last_error.clone_from(&error.detail);
                    c.warning(format!("{call}(): {}", error.detail));
                    return Ok(Value::Bool(false));
                }
                if OP == INSERT {
                    return result::create(c, rows, Some(&handle));
                }
                if OP == SELECT {
                    let result = result::create(c, rows, Some(&handle))?;
                    return crate::result::execute::<{ result::ALL }>(
                        c,
                        &[result, Value::Int(args::int(&a, 4).unwrap_or(1))],
                    );
                }
                Ok(Value::Bool(true))
            }
            Err(error) => {
                handle.borrow_mut().last_error.clone_from(&error.detail);
                c.warning(format!("{call}(): {}", error.detail));
                Ok(Value::Bool(false))
            }
        }
    })
}

fn joined(
    c: &mut dyn Context,
    fields: &[Field],
    separator: &str,
    predicate: bool,
) -> Result<String, phera_extension_api::NativeError> {
    let mut clauses = Vec::new();
    for field in fields {
        let value = to_php_string(&field.value, c.diagnostics())?.to_string_lossy();
        if predicate && value == "NULL" {
            clauses.push(format!("{} IS NULL", field.name));
        } else {
            clauses.push(format!("{}={value}", field.name));
        }
    }
    Ok(clauses.join(separator))
}
