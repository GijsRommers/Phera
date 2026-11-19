//! `mysqli_fetch_field($result)` returns the next column's protocol metadata.

use mysql_async::consts::{ColumnFlags, ColumnType};
use phera_db_drivers::metadata::Metadata;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value};

use super::{FIELD, FIELD_DIRECT, FIELD_SEEK, FIELD_TELL, FIELDS, ResultSet};

pub fn execute<const OP: usize>(
    context: &mut dyn Context,
    result: &mut ResultSet,
    index: i64,
    call: &str,
    arg: usize,
) -> NativeResult {
    let column = |index: usize| {
        result
            .rows
            .columns
            .get(index)
            .and_then(|column| match &column.metadata {
                Metadata::Mysql(column) => Some(column),
                _ => None,
            })
    };
    match OP {
        FIELD_TELL => Ok(Value::Int(result.field as i64)),
        FIELDS => {
            let mut fields = PhpArray::new();
            for index in 0..result.rows.columns.len() {
                if let Some(column) = column(index) {
                    fields.push(describe(context, column)?)?;
                }
            }
            Ok(Value::from(fields))
        }
        FIELD => {
            let Some(column) = column(result.field).cloned() else {
                return Ok(Value::Bool(false));
            };
            result.field += 1;
            describe(context, &column)
        }
        FIELD_DIRECT | FIELD_SEEK => {
            let index = usize::try_from(index).map_err(|_| {
                NativeError::value_error(format!(
                    "{call}(): Argument #{arg} ($index) must be greater than or equal to 0"
                ))
            })?;
            if column(index).is_none() {
                return Err(NativeError::value_error(format!(
                    "{call}(): Argument #{arg} ($index) must be less than the number of fields \
                     for this result set"
                )));
            }
            if OP == FIELD_SEEK {
                result.field = index;
                Ok(Value::Bool(true))
            } else {
                column(index).map_or(Ok(Value::Bool(false)), |col| describe(context, col))
            }
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn describe(context: &mut dyn Context, column: &mysql_async::Column) -> NativeResult {
    let mut flags = column.flags();
    let kind = column.column_type();
    // mysqlnd supplies NUM_FLAG for the historical protocol types.
    if kind == ColumnType::MYSQL_TYPE_YEAR
        || (kind as u8 <= ColumnType::MYSQL_TYPE_INT24 as u8
            && (kind != ColumnType::MYSQL_TYPE_TIMESTAMP
                || matches!(column.column_length(), 8 | 14)))
    {
        flags.insert(ColumnFlags::NUM_FLAG);
    }
    let mut fields = PhpArray::new();
    for (name, value) in [
        ("name", Value::from(column.name_ref().to_vec())),
        ("orgname", Value::from(column.org_name_ref().to_vec())),
        ("table", Value::from(column.table_ref().to_vec())),
        ("orgtable", Value::from(column.org_table_ref().to_vec())),
        ("def", Value::from("")),
        ("db", Value::from(column.schema_ref().to_vec())),
        ("catalog", Value::from("def")),
        ("max_length", Value::Int(0)),
        ("length", Value::Int(column.column_length().into())),
        ("charsetnr", Value::Int(column.character_set().into())),
        ("flags", Value::Int(flags.bits().into())),
        ("type", Value::Int(i64::from(column.column_type() as u8))),
        ("decimals", Value::Int(column.decimals().into())),
    ] {
        fields.insert(ArrayKey::str(name), value);
    }
    context.create_object("stdClass", fields)
}
