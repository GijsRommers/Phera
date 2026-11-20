//! PostgreSQL types follow PDO: `SELECT 1::int, true, 1.25::numeric` yields int, bool, string.

use std::error::Error;

use chrono::{Duration, NaiveDate};
use phera_extension_api::Value;
use tokio_postgres::types::{FromSql, Kind, Type};

use super::{Row, error};
use crate::drivers::{Column, RowBuffer, Rows};
use crate::errors::DbError;

type DecodeError = Box<dyn Error + Send + Sync>;

/// Turns PostgreSQL's rows into PHP values. A column PostgreSQL already
/// formatted as text takes that text; every other one is decoded from the
/// binary format.
pub fn convert(columns: Vec<Column>, rows: Vec<Row>, affected: u64) -> Result<Rows, DbError> {
    // Cursor batches carry rows only. The statement already has their columns.
    let width = rows.first().map_or(columns.len(), |row| row.binary.len());
    let mut values = RowBuffer::with_capacity(width, rows.len());
    for Row { binary, text } in rows {
        let mut text = text.into_iter().peekable();
        values.try_push((0..binary.len()).map(|index| {
            // `text` is in column order, so its next entry is this column's
            // or a later one's.
            let value = match text.next_if(|(column, _)| *column == index) {
                Some((_, text)) => Value::from(text),
                None => {
                    let cell: Option<Cell> = binary.try_get(index).map_err(|e| error(e, ""))?;
                    cell.map_or(Value::Null, |cell| cell.0)
                }
            };
            Ok::<_, DbError>(value)
        }))?;
    }
    Ok(Rows {
        columns,
        values,
        affected,
    })
}

struct Cell(Value);

impl<'a> FromSql<'a> for Cell {
    fn from_sql(ty: &Type, bytes: &'a [u8]) -> Result<Self, DecodeError> {
        decode(ty, bytes).map(Self)
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

fn decode(ty: &Type, bytes: &[u8]) -> Result<Value, DecodeError> {
    if let Kind::Array(element) = ty.kind() {
        return array(element, bytes).map(Value::from);
    }
    if let Kind::Domain(inner) = ty.kind() {
        return decode(inner, bytes);
    }
    let value = match *ty {
        Type::VOID => Value::from(""),
        Type::BOOL => Value::Bool(bool::from_sql(ty, bytes)?),
        Type::INT2 => Value::Int(i64::from(i16::from_sql(ty, bytes)?)),
        Type::INT4 => Value::Int(i64::from(i32::from_sql(ty, bytes)?)),
        Type::INT8 => Value::Int(i64::from_sql(ty, bytes)?),
        Type::OID => Value::Int(i64::from(u32::from_sql(ty, bytes)?)),
        Type::FLOAT4 => Value::Float(f64::from(f32::from_sql(ty, bytes)?)),
        Type::FLOAT8 => Value::Float(f64::from_sql(ty, bytes)?),
        Type::NUMERIC => Value::from(numeric(bytes)?),
        Type::BYTEA => return Err("PostgreSQL bytea streams are not supported yet".into()),
        Type::JSONB => Value::from(bytes.get(1..).ok_or("Invalid jsonb")?.to_vec()),
        Type::TIMESTAMP | Type::TIMESTAMPTZ => {
            let micros = i64::from_be_bytes(bytes.try_into()?);
            let text = match micros {
                i64::MAX => "infinity".into(),
                i64::MIN => "-infinity".into(),
                micros => {
                    let epoch = NaiveDate::from_ymd_opt(2000, 1, 1)
                        .and_then(|date| date.and_hms_opt(0, 0, 0))
                        .ok_or("Invalid epoch")?;
                    let date = epoch
                        .checked_add_signed(Duration::microseconds(micros))
                        .ok_or("Timestamp is out of range")?;
                    let mut text = date.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
                    text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
                    if *ty == Type::TIMESTAMPTZ {
                        text.push_str("+00");
                    }
                    text
                }
            };
            Value::from(text)
        }
        Type::DATE => {
            let days = i32::from_be_bytes(bytes.try_into()?);
            let epoch = NaiveDate::from_ymd_opt(2000, 1, 1).ok_or("Invalid epoch")?;
            Value::from(
                epoch
                    .checked_add_signed(Duration::days(i64::from(days)))
                    .ok_or("Date is out of range")?
                    .to_string(),
            )
        }
        Type::UUID => {
            if bytes.len() != 16 {
                return Err("Invalid uuid".into());
            }
            let mut text = String::new();
            for (index, byte) in bytes.iter().enumerate() {
                if [4, 6, 8, 10].contains(&index) {
                    text.push('-');
                }
                text.push_str(&format!("{byte:02x}"));
            }
            Value::from(text)
        }
        Type::CHAR => Value::from(bytes.strip_prefix(&[0]).unwrap_or(bytes).to_vec()),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::JSON | Type::XML => {
            Value::from(bytes.to_vec())
        }
        _ if matches!(ty.kind(), Kind::Enum(_)) => Value::from(bytes.to_vec()),
        _ => return Err(format!("PostgreSQL type {} is not supported yet", ty.name()).into()),
    };
    Ok(value)
}

fn numeric(bytes: &[u8]) -> Result<String, DecodeError> {
    let mut words = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_be_bytes([b[0], b[1]]));
    let count = words.next().ok_or("Invalid numeric")?;
    let weight = words.next().ok_or("Invalid numeric")?;
    let sign = words.next().ok_or("Invalid numeric")? as u16;
    let scale = words.next().ok_or("Invalid numeric")? as usize;
    match sign {
        0xc000 => return Ok("NaN".into()),
        0xd000 => return Ok("Infinity".into()),
        0xf000 => return Ok("-Infinity".into()),
        _ => {}
    }
    let digits: Vec<i16> = words.collect();
    if usize::try_from(count).ok() != Some(digits.len()) {
        return Err("Invalid numeric".into());
    }
    let digit = |power: i32| -> i16 {
        let index = i32::from(weight) - power;
        usize::try_from(index)
            .ok()
            .and_then(|i| digits.get(i))
            .copied()
            .unwrap_or(0)
    };
    let mut text = if sign == 0x4000 {
        "-".into()
    } else {
        String::new()
    };
    if weight < 0 {
        text.push('0');
    } else {
        for power in (0..=i32::from(weight)).rev() {
            if power == i32::from(weight) {
                text.push_str(&digit(power).to_string());
            } else {
                text.push_str(&format!("{:04}", digit(power)));
            }
        }
    }
    if scale > 0 {
        text.push('.');
        let start = text.len();
        for group in 1..=scale.div_ceil(4) {
            text.push_str(&format!("{:04}", digit(-(group as i32))));
        }
        text.truncate(start + scale);
    }
    Ok(text)
}

fn integer(bytes: &mut &[u8]) -> Result<i32, DecodeError> {
    let head = bytes.get(..4).ok_or("Truncated array")?;
    let number = i32::from_be_bytes(head.try_into()?);
    *bytes = &bytes[4..];
    Ok(number)
}

fn array(element: &Type, mut bytes: &[u8]) -> Result<String, DecodeError> {
    let dimensions = integer(&mut bytes)?;
    integer(&mut bytes)?;
    integer(&mut bytes)?;
    let mut lengths = Vec::new();
    let mut bounds = Vec::new();
    for _ in 0..dimensions {
        lengths.push(usize::try_from(integer(&mut bytes)?)?);
        bounds.push(integer(&mut bytes)?);
    }
    let mut prefix = String::new();
    if bounds.iter().any(|bound| *bound != 1) {
        for (length, bound) in lengths.iter().zip(bounds) {
            prefix.push_str(&format!(
                "[{bound}:{}]",
                i64::from(bound) + *length as i64 - 1
            ));
        }
        prefix.push('=');
    }
    prefix.push_str(&array_level(element, &lengths, &mut bytes)?);
    Ok(prefix)
}

fn array_level(
    element: &Type,
    lengths: &[usize],
    bytes: &mut &[u8],
) -> Result<String, DecodeError> {
    let Some((&length, remaining)) = lengths.split_first() else {
        return Ok("{}".into());
    };
    let mut items = Vec::new();
    for _ in 0..length {
        if !remaining.is_empty() {
            items.push(array_level(element, remaining, bytes)?);
            continue;
        }
        let size = integer(bytes)?;
        if size == -1 {
            items.push("NULL".into());
            continue;
        }
        let size = usize::try_from(size)?;
        let cell = bytes.get(..size).ok_or("Truncated array value")?;
        let value = decode(element, cell)?;
        *bytes = &bytes[size..];
        let text = match value {
            Value::String(value) => value.to_string_lossy(),
            Value::Int(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Bool(value) => if value { "t" } else { "f" }.into(),
            _ => return Err("Unsupported array value".into()),
        };
        let quote = text.is_empty()
            || text.eq_ignore_ascii_case("null")
            || text
                .chars()
                .any(|c| c.is_whitespace() || "{},\"\\".contains(c));
        items.push(if quote {
            format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
        } else {
            text
        });
    }
    Ok(format!("{{{}}}", items.join(",")))
}
