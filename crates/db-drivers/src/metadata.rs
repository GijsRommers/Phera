//! Result column information such as the type exposed by pg_field_type().

#[derive(Clone, Debug)]
pub enum Metadata {
    Postgres(PostgresColumn),
    Mysql(mysql_async::Column),
    Sqlite(phera_sqlite::Column, i64),
}

/// What PostgreSQL tells about a result column.
#[derive(Clone, Debug)]
pub struct PostgresColumn {
    pub oid: u32,
    pub table_oid: u32,
    pub modifier: i32,
    pub native_type: String,
}
