//! MySQL uses binary prepared statements, or text queries for `ATTR_EMULATE_PREPARES`.

use mysql_async::consts::ColumnType;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, Params, Row, Statement, Value as MysqlValue};
use phera_extension_api::Value;

use super::{Column, Prepared, Rows};
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::Sql;

pub struct Mysql {
    connection: Conn,
    pub version: String,
    last_insert_id: u64,
    pending_exec_rows: bool,
}

impl Mysql {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let port = dsn.port().map_err(|e| DbError {
            state: "HY000".into(),
            code: Some(2002),
            detail: format!("{e:?}"),
        })?;
        let options = OptsBuilder::default()
            .ip_or_hostname(dsn.get("host", "localhost"))
            .tcp_port(port)
            .user(Some(user))
            .pass(Some(password))
            .db_name(Some(dsn.get("dbname", "")))
            .prefer_socket(false)
            .socket(dsn.fields.get("unix_socket").cloned());
        let mut connection = Conn::new(options).await.map_err(|failure| {
            let mut failure = error(failure);
            failure.state = "HY000".into();
            failure
        })?;
        if let Some(charset) = dsn.fields.get("charset") {
            if !charset
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(DbError {
                    state: "HY000".into(),
                    code: Some(2019),
                    detail: "Unknown character set".into(),
                });
            }
            connection
                .query_drop(format!("SET NAMES {charset}"))
                .await
                .map_err(error)?;
        }
        let version: Option<String> = connection
            .query_first("SELECT VERSION()")
            .await
            .map_err(error)?;
        Ok(Self {
            connection,
            version: version.unwrap_or_default(),
            last_insert_id: 0,
            pending_exec_rows: false,
        })
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Statement, DbError> {
        self.check_pending()?;
        self.connection
            .prep(sql.render(|_| "?".into()))
            .await
            .map_err(error)
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
    ) -> Result<Rows, DbError> {
        self.check_pending()?;
        match prepared {
            Prepared::Mysql(statement) => {
                let params = Params::Positional(values.iter().map(parameter).collect());
                let mut result = self
                    .connection
                    .exec_iter(statement, params)
                    .await
                    .map_err(error)?;
                let columns = result.columns_ref().to_vec();
                let rows: Vec<Row> = result.collect().await.map_err(error)?;
                let affected = result.affected_rows();
                self.last_insert_id = result.last_insert_id().unwrap_or(0);
                result.drop_result().await.map_err(error)?;
                Ok(convert(columns, rows, affected))
            }
            Prepared::Emulated => {
                let mode = self.backslash_escapes().await?;
                let query = sql.render(|i| match &values[i] {
                    BoundValue::String(bytes) => quote(bytes, mode),
                    value => literal(value, true),
                });
                self.direct(&query).await
            }
            _ => Err(DbError {
                state: "HY000".into(),
                code: None,
                detail: "Mismatched database driver".into(),
            }),
        }
    }

    pub async fn direct(&mut self, sql: &str) -> Result<Rows, DbError> {
        self.check_pending()?;
        let mut result = self.connection.query_iter(sql).await.map_err(error)?;
        let columns = result.columns_ref().to_vec();
        let rows: Vec<Row> = result.collect().await.map_err(error)?;
        let affected = result.affected_rows();
        self.last_insert_id = result.last_insert_id().unwrap_or(0);
        result.drop_result().await.map_err(error)?;
        Ok(convert(columns, rows, affected))
    }

    pub async fn exec(&mut self, sql: &str) -> Result<Rows, DbError> {
        let rows = self.direct(sql).await?;
        // PHP exec() has no statement on which to consume a SELECT's result.
        self.pending_exec_rows = !rows.columns.is_empty();
        Ok(rows)
    }

    fn check_pending(&self) -> Result<(), DbError> {
        if !self.pending_exec_rows {
            return Ok(());
        }
        Err(DbError {
            state: "HY000".into(),
            code: Some(2014),
            detail: "Cannot execute queries while other unbuffered queries are active.  \
             Consider using PDOStatement::fetchAll().  Alternatively, if your code \
             is only ever going to run against mysql, you may enable query buffering \
             by setting the Pdo\\Mysql::ATTR_USE_BUFFERED_QUERY attribute."
                .into(),
        })
    }

    async fn backslash_escapes(&mut self) -> Result<bool, DbError> {
        let mode: Option<String> = self
            .connection
            .query_first("SELECT @@SESSION.sql_mode")
            .await
            .map_err(error)?;
        Ok(!mode
            .unwrap_or_default()
            .split(',')
            .any(|value| value == "NO_BACKSLASH_ESCAPES"))
    }

    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        let backslashes = self.backslash_escapes().await?;
        Ok(quote(bytes, backslashes))
    }

    pub fn last_id(&self) -> String {
        self.last_insert_id.to_string()
    }
}

fn parameter(value: &BoundValue) -> MysqlValue {
    match value {
        BoundValue::Null => MysqlValue::NULL,
        BoundValue::Int(value) => MysqlValue::Int(*value),
        BoundValue::Bool(value) => MysqlValue::Int(i64::from(*value)),
        BoundValue::String(value) => MysqlValue::Bytes(value.clone()),
    }
}

fn convert(columns: Vec<mysql_async::Column>, rows: Vec<Row>, affected: u64) -> Rows {
    let values: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|row| {
            row.unwrap()
                .into_iter()
                .zip(&columns)
                .map(|(value, column)| cell(value, column.column_type()))
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
            })
            .collect(),
        values,
        affected,
    }
}

fn cell(value: MysqlValue, kind: ColumnType) -> Value {
    match value {
        MysqlValue::NULL => Value::Null,
        MysqlValue::Int(value) => Value::Int(value),
        MysqlValue::UInt(value) => {
            i64::try_from(value).map_or_else(|_| Value::from(value.to_string()), Value::Int)
        }
        MysqlValue::Float(value) => Value::Float(f64::from(value)),
        MysqlValue::Double(value) => Value::Float(value),
        MysqlValue::Bytes(bytes) => {
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
                if micros > 0 {
                    text.push_str(&format!(".{micros:06}"));
                }
            }
            Value::from(text)
        }
        MysqlValue::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            let hours = u64::from(days) * 24 + u64::from(hours);
            let mut text = format!("{sign}{hours:02}:{minutes:02}:{seconds:02}");
            if micros > 0 {
                text.push_str(&format!(".{micros:06}"));
            }
            Value::from(text)
        }
    }
}

pub fn error(error: mysql_async::Error) -> DbError {
    match error {
        mysql_async::Error::Server(error) => DbError {
            state: error.state,
            code: Some(i64::from(error.code)),
            detail: error.message,
        },
        error => DbError {
            state: "HY000".into(),
            code: Some(2002),
            detail: error.to_string(),
        },
    }
}
