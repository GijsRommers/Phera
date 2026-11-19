//! Decode MySQL binary and text result values with PDO's scalar types.

use mysql_async::consts::ColumnType;
use mysql_async::{Row, Value as MysqlValue};
use phera_extension_api::Value;

use crate::drivers::{Column, Rows};
use crate::parameters::BoundValue;

pub fn parameter(value: &BoundValue) -> MysqlValue {
    match value {
        BoundValue::Null => MysqlValue::NULL,
        BoundValue::Int(value) => MysqlValue::Int(*value),
        BoundValue::Bool(value) => MysqlValue::Int(i64::from(*value)),
        BoundValue::String(value) | BoundValue::Blob(value) => MysqlValue::Bytes(value.clone()),
    }
}

pub fn convert(columns: Vec<mysql_async::Column>, rows: Vec<Row>, affected: u64) -> Rows {
    convert_rows(columns, rows, affected, NumericTypes::Pdo)
}

pub fn convert_mysqlnd(
    columns: Vec<mysql_async::Column>,
    rows: Vec<Row>,
    affected: u64,
    native: bool,
) -> Rows {
    convert_rows(columns, rows, affected, NumericTypes::Mysqlnd { native })
}

#[derive(Clone, Copy)]
enum NumericTypes {
    Pdo,
    Mysqlnd { native: bool },
}

fn convert_rows(
    columns: Vec<mysql_async::Column>,
    rows: Vec<Row>,
    affected: u64,
    numeric: NumericTypes,
) -> Rows {
    let values: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|row| {
            // `unwrap_raw` instead of `unwrap`, which panics for a value that
            // was taken out of the row; nothing here takes one.
            row.unwrap_raw()
                .into_iter()
                .map(|value| value.unwrap_or(MysqlValue::NULL))
                .zip(&columns)
                .map(|(value, column)| {
                    cell(value, column.column_type(), column.decimals(), numeric)
                })
                .collect()
        })
        .collect();
    let affected = if columns.is_empty() {
        affected
    } else {
        values.len() as u64
    };
    Rows {
        columns: columns
            .iter()
            .map(|column| Column {
                name: column.name_str().into_owned(),
                metadata: crate::metadata::Metadata::Mysql(column.clone()),
            })
            .collect(),
        values,
        affected,
    }
}

fn cell(value: MysqlValue, kind: ColumnType, decimals: u8, numeric: NumericTypes) -> Value {
    let native = !matches!(numeric, NumericTypes::Mysqlnd { native: false });
    let mysqlnd = matches!(numeric, NumericTypes::Mysqlnd { .. });
    match value {
        MysqlValue::Int(value) if mysqlnd && kind == ColumnType::MYSQL_TYPE_YEAR => {
            Value::from(format!("{value:04}"))
        }
        MysqlValue::UInt(value) if mysqlnd && kind == ColumnType::MYSQL_TYPE_YEAR => {
            Value::from(format!("{value:04}"))
        }
        MysqlValue::NULL => Value::Null,
        MysqlValue::Int(value) => Value::Int(value),
        MysqlValue::UInt(value) => {
            i64::try_from(value).map_or_else(|_| Value::from(value.to_string()), Value::Int)
        }
        MysqlValue::Float(value) => Value::Float(if mysqlnd {
            mysqlnd_float(value, decimals)
        } else {
            f64::from(value)
        }),
        MysqlValue::Double(value) => Value::Float(value),
        MysqlValue::Bytes(bytes) => {
            if kind == ColumnType::MYSQL_TYPE_BIT {
                let number = bytes
                    .iter()
                    .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
                if !native {
                    return Value::from(number.to_string());
                }
                return i64::try_from(number)
                    .map_or_else(|_| Value::from(number.to_string()), Value::Int);
            }
            if !native || (mysqlnd && kind == ColumnType::MYSQL_TYPE_YEAR) {
                return Value::from(bytes);
            }
            let text = String::from_utf8_lossy(&bytes);
            match kind {
                ColumnType::MYSQL_TYPE_TINY
                | ColumnType::MYSQL_TYPE_SHORT
                | ColumnType::MYSQL_TYPE_LONG
                | ColumnType::MYSQL_TYPE_LONGLONG
                | ColumnType::MYSQL_TYPE_INT24
                | ColumnType::MYSQL_TYPE_YEAR => {
                    if let Ok(number) = text.parse::<i64>() {
                        return Value::Int(number);
                    }
                }
                ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => {
                    if let Ok(number) = text.parse::<f64>() {
                        return Value::Float(number);
                    }
                }
                _ => {}
            }
            Value::from(bytes)
        }
        MysqlValue::Date(year, month, day, hour, minute, second, micros) => {
            let mut text = format!("{year:04}-{month:02}-{day:02}");
            if kind != ColumnType::MYSQL_TYPE_DATE {
                text.push_str(&format!(" {hour:02}:{minute:02}:{second:02}"));
                fraction(&mut text, micros, decimals);
            }
            Value::from(text)
        }
        MysqlValue::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            let hours = u64::from(days) * 24 + u64::from(hours);
            let mut text = format!("{sign}{hours:02}:{minutes:02}:{seconds:02}");
            fraction(&mut text, micros, decimals);
            Value::from(text)
        }
    }
}

fn fraction(text: &mut String, micros: u32, decimals: u8) {
    if decimals > 0 && decimals <= 6 {
        let digits = format!("{micros:06}");
        text.push('.');
        text.push_str(&digits[..usize::from(decimals)]);
    }
}

// mysqlnd widens a binary FLOAT through decimal text, with six significant digits
// for columns without a fixed decimal scale.
fn mysqlnd_float(value: f32, decimals: u8) -> f64 {
    let value = f64::from(value);
    let text = if decimals < 31 {
        format!("{value:.precision$}", precision = usize::from(decimals))
    } else {
        format!("{value:.5e}")
    };
    text.parse().unwrap_or(value)
}
