//! `$statement->getColumnMeta(0)` exposes the driver's result-column description.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use tokio_postgres::types::Type;

use crate::constants::{PARAM_BOOL, PARAM_INT, PARAM_LOB, PARAM_STR};
use crate::drivers::Column;
use crate::statement::state;

#[derive(Clone, Debug)]
pub enum Metadata {
    Postgres(PostgresColumn),
    Mysql(mysql_async::Column),
    Sqlite(phera_sqlite::Column, i64),
}

/// What tokio-postgres tells about a result column.
#[derive(Clone, Debug)]
pub struct PostgresColumn {
    pub oid: u32,
    pub table_oid: u32,
    pub modifier: i32,
    pub native_type: String,
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
        if matches!(
            statement.borrow().prepared,
            crate::drivers::Prepared::Sqlite(_)
        ) && !statement.borrow().sqlite_executed
        {
            return Ok(Value::Bool(false));
        }
        let (connection, column) = {
            let statement = statement.borrow();
            (
                statement.connection.clone(),
                statement.rows.columns.get(index).cloned(),
            )
        };
        let Some(column) = column else {
            if connection.name == crate::dsn::DriverName::Sqlite {
                let active = match &statement.borrow().prepared {
                    crate::drivers::Prepared::Sqlite(handle) => handle.busy(),
                    _ => false,
                };
                return connection.fail(
                    context,
                    crate::errors::DbError {
                        state: "HY000".into(),
                        code: Some(if active { 100 } else { 25 }),
                        detail: if active {
                            "another row available"
                        } else {
                            "column index out of range"
                        }
                        .into(),
                    },
                    "PDOStatement::getColumnMeta",
                );
            }
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
        | ColumnType::MYSQL_TYPE_BIT => PARAM_INT,
        _ => PARAM_STR,
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
        metadata: Metadata::Postgres(PostgresColumn {
            oid: column.type_().oid(),
            table_oid: column.table_oid().unwrap_or(0),
            modifier: column.type_modifier(),
            native_type: column.type_().name().into(),
        }),
    }
}

pub fn insert(array: &mut PhpArray, name: &str, value: Value) {
    array.insert(ArrayKey::str(name), value);
}

/// `getColumnMeta()` for PostgreSQL, with the type length and table name
/// the catalog lookup found.
pub fn postgres_values(
    name: &str,
    column: &PostgresColumn,
    length: i16,
    table: Option<String>,
) -> PhpArray {
    let PostgresColumn {
        oid,
        table_oid,
        modifier,
        native_type,
    } = column;
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
    let pdo_type = match Type::from_oid(*oid) {
        Some(Type::BOOL) => PARAM_BOOL,
        Some(Type::INT8 | Type::INT2 | Type::INT4 | Type::OID) => PARAM_INT,
        Some(Type::BYTEA) => PARAM_LOB,
        _ => PARAM_STR,
    };
    insert(&mut values, "pdo_type", Value::Int(pdo_type));
    insert(&mut values, "name", Value::from(name));
    insert(&mut values, "len", Value::Int(i64::from(length)));
    insert(&mut values, "precision", Value::Int(i64::from(*modifier)));
    values
}
