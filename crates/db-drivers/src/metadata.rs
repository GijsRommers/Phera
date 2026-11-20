//! Result column information such as the type exposed by pg_field_type().

use crate::drivers::Column;

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
