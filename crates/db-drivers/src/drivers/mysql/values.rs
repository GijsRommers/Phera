//! Decode MySQL binary and text result values with PDO's scalar types.

use mysql_async::consts::ColumnType;
use mysql_async::{Row, Value as MysqlValue};
use phera_extension_api::Value;

use crate::drivers::{Column, RowBuffer, Rows};
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
    let mut values = RowBuffer::with_capacity(columns.len(), rows.len());
    for row in rows {
        push_row(&mut values, row, &columns, numeric);
    }
    finish(columns, values, affected)
}

pub(super) fn push_pdo_row(values: &mut RowBuffer, row: Row, columns: &[mysql_async::Column]) {
    push_row(values, row, columns, NumericTypes::Pdo);
}

fn push_row(
    values: &mut RowBuffer,
    row: Row,
    columns: &[mysql_async::Column],
    numeric: NumericTypes,
) {
    values.push(
        row.unwrap_raw()
            .into_iter()
            .zip(columns)
            .map(|(value, column)| {
                cell(
                    value.unwrap_or(MysqlValue::NULL),
                    column.column_type(),
                    column.decimals(),
                    numeric,
                )
            }),
    );
}

pub(super) fn finish(columns: Vec<mysql_async::Column>, values: RowBuffer, affected: u64) -> Rows {
    let affected = if columns.is_empty() {
        affected
    } else {
        values.len() as u64
    };
    Rows {
        columns: columns
            .into_iter()
            .map(|column| Column {
                name: column.name_str().into_owned(),
                metadata: crate::metadata::Metadata::Mysql(column),
            })
            .collect(),
        values,
        affected,
        has_pending_rows: false,
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
            match kind {
                ColumnType::MYSQL_TYPE_TINY
                | ColumnType::MYSQL_TYPE_SHORT
                | ColumnType::MYSQL_TYPE_LONG
                | ColumnType::MYSQL_TYPE_LONGLONG
                | ColumnType::MYSQL_TYPE_INT24
                | ColumnType::MYSQL_TYPE_YEAR => {
                    if let Ok(text) = std::str::from_utf8(&bytes)
                        && let Ok(number) = text.parse::<i64>()
                    {
                        return Value::Int(number);
                    }
                }
                ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => {
                    if let Ok(text) = std::str::from_utf8(&bytes)
                        && let Ok(number) = text.parse::<f64>()
                    {
                        return Value::Float(number);
                    }
                }
                _ => {}
            }
            Value::from(bytes)
        }
        MysqlValue::Date(year, month, day, hour, minute, second, micros) => {
            let mut buf = [0u8; 32];
            let mut len = write_date(&mut buf, year, month, day);
            if kind != ColumnType::MYSQL_TYPE_DATE {
                buf[len] = b' ';
                len += 1;
                len += write_two(&mut buf[len..], hour);
                buf[len] = b':';
                len += 1;
                len += write_two(&mut buf[len..], minute);
                buf[len] = b':';
                len += 1;
                len += write_two(&mut buf[len..], second);
                len += write_fraction(&mut buf[len..], micros, decimals);
            }
            Value::from(&buf[..len])
        }
        MysqlValue::Time(negative, days, hours, minutes, seconds, micros) => {
            let mut buf = [0u8; 32];
            let mut len = 0;
            if negative {
                buf[len] = b'-';
                len += 1;
            }
            let total_hours = u64::from(days) * 24 + u64::from(hours);
            len += write_hours(&mut buf[len..], total_hours);
            buf[len] = b':';
            len += 1;
            len += write_two(&mut buf[len..], minutes);
            buf[len] = b':';
            len += 1;
            len += write_two(&mut buf[len..], seconds);
            len += write_fraction(&mut buf[len..], micros, decimals);
            Value::from(&buf[..len])
        }
    }
}

#[inline]
fn write_two(buf: &mut [u8], value: u8) -> usize {
    buf[0] = b'0' + (value / 10);
    buf[1] = b'0' + (value % 10);
    2
}

#[inline]
fn write_date(buf: &mut [u8], year: u16, month: u8, day: u8) -> usize {
    buf[0] = b'0' + ((year / 1000) % 10) as u8;
    buf[1] = b'0' + ((year / 100) % 10) as u8;
    buf[2] = b'0' + ((year / 10) % 10) as u8;
    buf[3] = b'0' + (year % 10) as u8;
    buf[4] = b'-';
    write_two(&mut buf[5..], month);
    buf[7] = b'-';
    write_two(&mut buf[8..], day);
    10
}

#[inline]
fn write_hours(buf: &mut [u8], hours: u64) -> usize {
    if hours < 100 {
        return write_two(buf, hours as u8);
    }
    let mut tmp = [0u8; 20];
    let mut n = hours;
    let mut count = 0;
    while n > 0 {
        tmp[count] = b'0' + (n % 10) as u8;
        n /= 10;
        count += 1;
    }
    for i in 0..count {
        buf[i] = tmp[count - 1 - i];
    }
    count
}

#[inline]
fn write_fraction(buf: &mut [u8], micros: u32, decimals: u8) -> usize {
    if decimals == 0 || decimals > 6 {
        return 0;
    }
    buf[0] = b'.';
    let mut n = micros;
    let mut digits = [b'0'; 6];
    for slot in digits.iter_mut().rev() {
        *slot = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let d = usize::from(decimals);
    buf[1..1 + d].copy_from_slice(&digits[..d]);
    1 + d
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
