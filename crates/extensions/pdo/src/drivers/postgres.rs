//! PostgreSQL prepares use `$1` on the wire: `$pdo->prepare('SELECT ?::int')`.

use std::error::Error;

use bytes::BytesMut;
use phera_extension_api::Value;
use tokio_postgres::types::{Format, IsNull, ToSql, Type};
use tokio_postgres::{Client, Config, NoTls, SimpleQueryMessage, Statement};

use super::{Column, Prepared, Rows, connection_error};
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::Sql;

pub struct Postgres {
    client: Client,
    task: tokio::task::JoinHandle<()>,
    pub version: String,
}

impl Drop for Postgres {
    fn drop(&mut self) {
        // Closing the socket causes PostgreSQL to roll back any open transaction.
        self.task.abort();
    }
}

impl Postgres {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let mut config = Config::new();
        config
            .host(dsn.get("host", "localhost"))
            .port(dsn.port().map_err(|e| connection_error(format!("{e:?}")))?)
            .dbname(dsn.get("dbname", user))
            .user(user)
            .password(password)
            .ssl_mode(tokio_postgres::config::SslMode::Disable);
        let (client, connection) = config.connect(NoTls).await.map_err(|error| {
            let detail = match error.as_db_error() {
                Some(database) => format!("{}:  {}", database.severity(), database.message()),
                None => error.to_string(),
            };
            connection_error(format!(
                "connection to server at \"{}\", port {} failed: {detail}",
                dsn.get("host", "localhost"),
                dsn.get("port", "5432")
            ))
        })?;
        let task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut driver = Self {
            client,
            task,
            version: String::new(),
        };
        let messages = driver
            .client
            .simple_query("SHOW server_version")
            .await
            .map_err(|e| error(e, "SHOW server_version"))?;
        for message in messages {
            if let SimpleQueryMessage::Row(row) = message {
                driver.version = row.get(0).unwrap_or_default().into();
            }
        }
        Ok(driver)
    }

    pub async fn prepare(&self, sql: &Sql) -> Result<Statement, DbError> {
        let query = sql.render(|i| format!("${}", i + 1));
        self.client
            .prepare(&query)
            .await
            .map_err(|e| error(e, &query))
    }

    pub async fn execute(
        &self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
    ) -> Result<Rows, DbError> {
        let statement = match prepared {
            Prepared::Postgres(statement) => statement.clone(),
            Prepared::PostgresDeferred => self.prepare(sql).await?,
            Prepared::Emulated => {
                let mut literals = Vec::new();
                for value in values {
                    literals.push(match value {
                        BoundValue::String(bytes) => self.quote(bytes).await?,
                        value => literal(value, false),
                    });
                }
                let rendered = sql.render(|i| literals[i].clone());
                self.client
                    .prepare(&rendered)
                    .await
                    .map_err(|e| error(e, &rendered))?
            }
            _ => return Err(connection_error("Mismatched database driver")),
        };
        let parameters: Vec<TextParameter> = if matches!(prepared, Prepared::Emulated) {
            Vec::new()
        } else {
            values.iter().map(TextParameter::from).collect()
        };
        let references: Vec<&(dyn ToSql + Sync)> = parameters
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect();
        let query = sql.render(|i| format!("${}", i + 1));
        let mut result = Rows {
            columns: statement
                .columns()
                .iter()
                .map(|column| Column {
                    name: column.name().into(),
                })
                .collect(),
            ..Rows::default()
        };
        if statement.columns().is_empty() {
            result.affected = self
                .client
                .execute(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
        } else {
            let rows = self
                .client
                .query(&statement, &references)
                .await
                .map_err(|e| error(e, &query))?;
            result.affected = rows.len() as u64;
            for row in rows {
                let mut values = Vec::new();
                for index in 0..row.len() {
                    let value = row
                        .try_get::<_, Option<super::postgres_values::Cell>>(index)
                        .map_err(|e| error(e, &query))?;
                    values.push(value.map_or(Value::Null, |cell| cell.0));
                }
                result.values.push(values);
            }
        }
        Ok(result)
    }

    pub async fn direct(&self, sql: &str) -> Result<Rows, DbError> {
        let messages = self
            .client
            .simple_query(sql)
            .await
            .map_err(|e| error(e, sql))?;
        let mut result = Rows::default();
        let mut returns_rows = false;
        for message in messages {
            match message {
                SimpleQueryMessage::RowDescription(_) => returns_rows = true,
                SimpleQueryMessage::CommandComplete(count) => {
                    result.affected = if returns_rows { 0 } else { count };
                    returns_rows = false;
                }
                _ => {}
            }
        }
        Ok(result)
    }

    pub async fn quote(&self, bytes: &[u8]) -> Result<String, DbError> {
        let rows = self
            .client
            .simple_query("SHOW standard_conforming_strings")
            .await
            .map_err(|e| error(e, ""))?;
        let escape = rows.iter().any(|row| {
            matches!(row,
            SimpleQueryMessage::Row(row) if row.get(0) == Some("off"))
        });
        let quoted = quote(bytes, false);
        Ok(if escape {
            quoted.replace('\\', "\\\\")
        } else {
            quoted
        })
    }

    pub async fn last_id(&self, name: Option<&str>) -> Result<String, DbError> {
        let row = match name {
            Some(name) => {
                self.client
                    .query_one("SELECT currval($1::text)", &[&name])
                    .await
            }
            None => self.client.query_one("SELECT lastval()", &[]).await,
        }
        .map_err(|e| error(e, ""))?;
        let id: i64 = row.try_get(0).map_err(|e| error(e, ""))?;
        Ok(id.to_string())
    }
}

#[derive(Debug)]
struct TextParameter(Option<Vec<u8>>);

impl From<&BoundValue> for TextParameter {
    fn from(value: &BoundValue) -> Self {
        Self(match value {
            BoundValue::Null => None,
            BoundValue::String(value) => Some(value.clone()),
            BoundValue::Int(value) => Some(value.to_string().into_bytes()),
            BoundValue::Bool(value) => Some(if *value { b"t" } else { b"f" }.to_vec()),
        })
    }
}

impl ToSql for TextParameter {
    fn to_sql(
        &self,
        _: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match &self.0 {
            None => Ok(IsNull::Yes),
            Some(bytes) => {
                output.extend_from_slice(bytes);
                Ok(IsNull::No)
            }
        }
    }
    fn accepts(_: &Type) -> bool {
        true
    }
    fn encode_format(&self, _: &Type) -> Format {
        Format::Text
    }
    fn to_sql_checked(
        &self,
        ty: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        self.to_sql(ty, output)
    }
}

pub fn error(error: tokio_postgres::Error, query: &str) -> DbError {
    let Some(database) = error.as_db_error() else {
        if let Some(source) = error.source() {
            let detail = source.to_string();
            if detail.contains("not supported yet") {
                return DbError {
                    state: "HY000".into(),
                    code: None,
                    detail,
                };
            }
        }
        return connection_error(error);
    };
    let mut detail = format!("{}:  {}", database.severity(), database.message());
    if let Some(tokio_postgres::error::ErrorPosition::Original(position)) = database.position() {
        let offset = (*position as usize).saturating_sub(1);
        let prefix: String = query.chars().take(offset).collect();
        let line_number = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count();
        let line = query.lines().nth(line_number - 1).unwrap_or_default();
        let label = format!("LINE {line_number}: ");
        detail.push_str(&format!(
            "\n{label}{line}\n{}^",
            " ".repeat(label.len() + column)
        ));
    }
    if let Some(extra) = database.detail() {
        detail.push_str(&format!("\nDETAIL:  {extra}"));
    }
    if let Some(hint) = database.hint() {
        detail.push_str(&format!("\nHINT:  {hint}"));
    }
    DbError {
        state: database.code().code().into(),
        code: Some(7),
        detail,
    }
}
