//! `pg_meta_data($db, 'public.orders')` reads column descriptions from PostgreSQL catalogs.

use phera_db_drivers::drivers::postgres::NativeResult;
use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::escape;

pub fn query(table: &str) -> String {
    let (schema, table) = table.split_once('.').unwrap_or(("public", table));
    let schema = escape::literal(schema.trim_matches('"'));
    let table = escape::literal(table.trim_matches('"'));
    format!(
        "SELECT a.attname, a.attnum, t.typname, a.attlen, a.attnotnull, a.atthasdef, \
        a.attndims, t.typtype, coalesce(pg_catalog.col_description(c.oid, a.attnum), '') \
        FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON a.attrelid=c.oid \
        JOIN pg_catalog.pg_namespace n ON c.relnamespace=n.oid \
        JOIN pg_catalog.pg_type t ON t.oid=a.atttypid \
        WHERE c.relname={table} AND n.nspname={schema} AND a.attnum>0 AND NOT a.attisdropped \
        ORDER BY a.attnum"
    )
}

pub struct Column {
    pub name: String,
    pub number: i64,
    pub type_name: String,
    pub length: i64,
    pub not_null: bool,
    pub default: bool,
    dims: i64,
    kind: String,
    description: String,
}

pub fn columns(result: NativeResult) -> Vec<Column> {
    result
        .values
        .into_iter()
        .map(|row| {
            let mut fields = row.into_iter().map(|value| {
                value.map_or_else(String::new, |value| {
                    String::from_utf8_lossy(&value).into_owned()
                })
            });
            let name = fields.next().unwrap_or_default();
            let number = fields
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_default();
            let type_name = fields.next().unwrap_or_default();
            let length = fields.next().and_then(|v| v.parse().ok()).unwrap_or(-1);
            let not_null = fields.next().as_deref() == Some("t");
            let default = fields.next().as_deref() == Some("t");
            let dims = fields
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_default();
            let kind = fields.next().unwrap_or_default();
            let description = fields.next().unwrap_or_default();
            Column {
                name,
                number,
                type_name,
                length,
                not_null,
                default,
                dims,
                kind,
                description,
            }
        })
        .collect()
}

pub fn to_array(columns: Vec<Column>, extended: bool) -> Value {
    let mut result = PhpArray::new();
    for col in columns {
        let mut fields = PhpArray::new();
        for (key, value) in [
            ("num", Value::Int(col.number)),
            ("type", Value::from(col.type_name)),
            ("len", Value::Int(col.length)),
            ("not null", Value::Bool(col.not_null)),
            ("has default", Value::Bool(col.default)),
            ("array dims", Value::Int(col.dims)),
            ("is enum", Value::Bool(col.kind == "e")),
        ] {
            fields.insert(ArrayKey::str(key), value);
        }
        if extended {
            for (key, value) in [
                ("is base", Value::Bool(col.kind == "b")),
                ("is composite", Value::Bool(col.kind == "c")),
                ("is pseudo", Value::Bool(col.kind == "p")),
                ("description", Value::from(col.description)),
            ] {
                fields.insert(ArrayKey::str(key), value);
            }
        }
        result.insert(ArrayKey::str(col.name), Value::from(fields));
    }
    Value::from(result)
}
