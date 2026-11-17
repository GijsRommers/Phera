//! `$statement->getColumnMeta(0)` exposes the driver's result-column description.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::drivers::Column;
use crate::statement::state;

#[derive(Clone, Debug)]
pub enum Metadata {
    Postgres {
        oid: u32,
        table_oid: u32,
        modifier: i32,
        native_type: String,
    },
    Mysql(mysql_async::Column),
}

pub fn get<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let index = usize::try_from(args::int(&arguments, 0).unwrap_or(0)).map_err(|_| {
            NativeError::value_error(
                "PDOStatement::getColumnMeta(): Argument #1 ($column) \
                 must be greater than or equal to 0",
            )
        })?;
        let statement = state(this.as_ref())?;
        let (connection, column) = {
            let statement = statement.borrow();
            (
                statement.connection.clone(),
                statement.rows.columns.get(index).cloned(),
            )
        };
        let Some(column) = column else {
            return Ok(Value::Bool(false));
        };
        let result = connection.driver.lock().await.metadata(&column).await;
        match result {
            Ok(values) => Ok(Value::from(values)),
            Err(error) => connection.fail(context, error, "PDOStatement::getColumnMeta"),
        }
    })
}

pub fn mysql(column: &mysql_async::Column) -> PhpArray {
    use mysql_async::consts::{ColumnFlags, ColumnType};
    let kind = column.column_type();
    let native = format!("{kind:?}")
        .trim_start_matches("MYSQL_TYPE_")
        .to_owned();
    let pdo_type = match kind {
        ColumnType::MYSQL_TYPE_TINY
        | ColumnType::MYSQL_TYPE_SHORT
        | ColumnType::MYSQL_TYPE_LONG
        | ColumnType::MYSQL_TYPE_LONGLONG
        | ColumnType::MYSQL_TYPE_INT24
        | ColumnType::MYSQL_TYPE_YEAR
        | ColumnType::MYSQL_TYPE_BIT => 1,
        _ => 2,
    };
    let mut values = PhpArray::new();
    insert(&mut values, "native_type", Value::from(native));
    insert(&mut values, "pdo_type", Value::Int(pdo_type));
    let mut flags = PhpArray::new();
    for (flag, name) in [
        (ColumnFlags::NOT_NULL_FLAG, "not_null"),
        (ColumnFlags::PRI_KEY_FLAG, "primary_key"),
        (ColumnFlags::UNIQUE_KEY_FLAG, "unique_key"),
        (ColumnFlags::MULTIPLE_KEY_FLAG, "multiple_key"),
        (ColumnFlags::BLOB_FLAG, "blob"),
    ] {
        if column.flags().contains(flag) {
            flags.insert(ArrayKey::Int(flags.len() as i64), Value::from(name));
        }
    }
    insert(&mut values, "flags", Value::from(flags));
    insert(
        &mut values,
        "table",
        Value::from(column.table_str().into_owned()),
    );
    insert(
        &mut values,
        "name",
        Value::from(column.name_str().into_owned()),
    );
    insert(
        &mut values,
        "len",
        Value::Int(i64::from(column.column_length())),
    );
    insert(
        &mut values,
        "precision",
        Value::Int(i64::from(column.decimals())),
    );
    values
}

pub fn postgres(column: &tokio_postgres::Column) -> Column {
    Column {
        name: column.name().into(),
        metadata: Metadata::Postgres {
            oid: column.type_().oid(),
            table_oid: column.table_oid().unwrap_or(0),
            modifier: column.type_modifier(),
            native_type: column.type_().name().into(),
        },
    }
}

pub fn insert(array: &mut PhpArray, name: &str, value: Value) {
    array.insert(ArrayKey::str(name), value);
}

pub fn postgres_values(column: &Column, length: i16, table: Option<String>) -> PhpArray {
    let Metadata::Postgres {
        oid,
        table_oid,
        modifier,
        native_type,
    } = &column.metadata
    else {
        unreachable!("PostgreSQL metadata only")
    };
    let mut values = PhpArray::new();
    insert(&mut values, "pgsql:oid", Value::Int(i64::from(*oid)));
    insert(
        &mut values,
        "pgsql:table_oid",
        Value::Int(i64::from(*table_oid)),
    );
    if let Some(table) = table {
        insert(&mut values, "table", Value::from(table));
    }
    insert(&mut values, "native_type", Value::from(native_type.clone()));
    let pdo_type = match *oid {
        16 => 5,
        20 | 21 | 23 | 26 => 1,
        17 => 3,
        _ => 2,
    };
    insert(&mut values, "pdo_type", Value::Int(pdo_type));
    insert(&mut values, "name", Value::from(column.name.clone()));
    insert(&mut values, "len", Value::Int(i64::from(length)));
    insert(&mut values, "precision", Value::Int(i64::from(*modifier)));
    values
}
