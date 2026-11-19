//! `pg_field_type($result, 0)` reports the type OID and name from PostgreSQL.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use super::{ResultSet, closed, of};
use crate::connection;

pub const NAME: usize = 0;
pub const SIZE: usize = 1;
pub const TYPE: usize = 2;
pub const TYPE_OID: usize = 3;
pub const NUM: usize = 4;

pub fn index(result: &ResultSet, field: i64, call: &str, arg: usize) -> Result<usize, NativeError> {
    usize::try_from(field).ok().filter(|field| *field < result.rows.columns.len())
        .ok_or_else(|| NativeError::value_error(format!(
            "{call}(): Argument #{arg} ($field) must be less than the number of fields for this result"
        )))
}

pub fn sync<const OP: usize>(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let state = of(arguments)?;
    let state = state.borrow();
    let state = state.as_ref().ok_or_else(closed)?;
    if OP == NUM {
        let name = args::bytes(arguments, 1);
        let found = state
            .rows
            .columns
            .iter()
            .position(|col| col.name.as_bytes() == name);
        return Ok(Value::Int(found.map_or(-1, |i| i as i64)));
    }
    let names = [
        "pg_field_name",
        "pg_field_size",
        "pg_field_type",
        "pg_field_type_oid",
    ];
    let name = names.get(OP).copied().unwrap_or_default();
    let field = index(state, args::int(arguments, 1).unwrap_or_default(), name, 2)?;
    let Some(column) = state.rows.columns.get(field) else {
        return Ok(Value::Bool(false));
    };
    Ok(match OP {
        NAME => Value::from(column.name.clone()),
        SIZE => Value::Int(column.size.into()),
        TYPE => Value::from(column.kind.clone()),
        TYPE_OID => Value::Int(column.oid.into()),
        _ => Value::Bool(false),
    })
}

pub fn table(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = of(&arguments)?;
        let (oid, connection) = {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(closed)?;
            let index = index(
                state,
                args::int(&arguments, 1).unwrap_or_default(),
                "pg_field_table",
                2,
            )?;
            let oid = state
                .rows
                .columns
                .get(index)
                .map_or(0, |column| column.table);
            (
                oid,
                state.connection.as_ref().and_then(std::rc::Weak::upgrade),
            )
        };
        if oid == 0 {
            return Ok(Value::Bool(false));
        }
        if args::bool(&arguments, 2).unwrap_or_default() {
            return Ok(Value::Int(oid.into()));
        }
        let Some(connection) = connection else {
            return Ok(Value::Bool(false));
        };
        let lease = connection.borrow().lease()?;
        let query = format!("SELECT relname FROM pg_catalog.pg_class WHERE oid = {oid}");
        let output = connection::run(&lease, move |pg| {
            Box::pin(async move { pg.native_query(&query).await })
        })
        .await
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .and_then(|rows| rows.values.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .flatten();
        Ok(output.map_or(Value::Bool(false), Value::from))
    })
}
