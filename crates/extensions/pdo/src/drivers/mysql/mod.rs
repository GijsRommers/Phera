//! MySQL uses binary prepared statements, or text queries for `ATTR_EMULATE_PREPARES`.

use std::sync::Arc;
use std::time::Duration;

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, Params, Row, Statement};

mod information;
mod stream;
mod tls;
pub mod values;

use super::{Execution, Prepared, RawRows};
use crate::connect_options::MysqlOptions;
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::Sql;
use values::parameter;

pub struct Mysql {
    connection: Option<Conn>,
    active: Option<stream::Active>,
    pub version: String,
    last_insert_id: u64,
    pending_exec_rows: bool,
    charset: Option<String>,
    database: String,
    options: MysqlOptions,
    host: String,
    unix_socket: bool,
}

impl Mysql {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let port = dsn.port()?;
        let settings = &dsn.options.mysql;
        let connect = async {
            let mut options = OptsBuilder::default()
                .ip_or_hostname(dsn.get("host", "localhost"))
                .tcp_port(port)
                .user(Some(user))
                .pass(Some(password))
                .db_name(Some(dsn.get("dbname", "")))
                .prefer_socket(false)
                .socket(dsn.fields.get("unix_socket").cloned())
                .client_found_rows(settings.found_rows);
            if settings.compress {
                options = options.compression(mysql_async::Compression::default());
            }
            if !settings.multiple {
                options = options.stmt_cache_size(0);
            }
            if let Some(tls) = &settings.ssl {
                options = options.ssl_opts(tls::options(tls).await?);
            }
            Conn::new(options).await.map_err(|failure| {
                if settings.ssl.is_some() && !matches!(failure, mysql_async::Error::Server(_)) {
                    return tls::failure();
                }
                let mut failure = error(failure);
                failure.state = "HY000".into();
                failure
            })
        };
        let seconds = dsn.options.timeout.unwrap_or(30) as u32;
        let mut connection = if seconds == 0 {
            connect.await?
        } else {
            tokio::time::timeout(Duration::from_secs(u64::from(seconds)), connect)
                .await
                .map_err(|_| DbError {
                    state: "HY000".into(),
                    code: Some(2002),
                    detail: "Connection timed out".into(),
                })??
        };
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
        configure(&mut connection, settings).await?;
        Ok(Self {
            connection: Some(connection),
            active: None,
            version: version.unwrap_or_default(),
            last_insert_id: 0,
            pending_exec_rows: false,
            charset: dsn.fields.get("charset").cloned(),
            database: dsn.get("dbname", "").to_owned(),
            options: settings.clone(),
            host: dsn.get("host", "localhost").to_owned(),
            unix_socket: dsn.fields.contains_key("unix_socket"),
        })
    }

    pub async fn close(mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.disconnect().await;
        }
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        self.finish_stream().await;
        let connection = self.connection.as_mut().ok_or_else(Self::closed)?;
        if !connection.reset().await.map_err(error)? {
            connection
                .change_user(Default::default())
                .await
                .map_err(error)?;
        }
        // COM_RESET_CONNECTION does not restore the database selected with USE.
        if !self.database.is_empty() {
            let database = self.database.replace('`', "``");
            connection
                .query_drop(format!("USE `{database}`"))
                .await
                .map_err(error)?;
        }
        if let Some(charset) = &self.charset {
            connection
                .query_drop(format!("SET NAMES {charset}"))
                .await
                .map_err(error)?;
        }
        configure(connection, &self.options).await?;
        self.last_insert_id = 0;
        self.pending_exec_rows = false;
        Ok(())
    }

    pub fn is_broken(&self) -> bool {
        self.active.is_none() && self.connection.as_ref().is_none_or(Conn::is_disconnected)
    }

    pub async fn health(&mut self) -> Result<(), DbError> {
        self.connection()?.ping().await.map_err(error)
    }

    pub async fn set_autocommit(&mut self, enabled: bool) -> Result<(), DbError> {
        self.check_pending().await?;
        self.connection()?
            .query_drop(if enabled {
                "SET autocommit=1"
            } else {
                "SET autocommit=0"
            })
            .await
            .map_err(error)
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Statement, DbError> {
        self.check_pending().await?;
        self.connection()?
            .prep(sql.render(|_| "?".into()))
            .await
            .map_err(error)
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: &Execution,
    ) -> Result<RawRows, DbError> {
        self.check_pending().await?;
        match prepared {
            Prepared::Mysql(statement) => {
                let params = Params::Positional(values.iter().map(parameter).collect());
                if let Some(owner) = &options.mysql_stream {
                    return self
                        .stream(stream::Query::Binary(statement.clone(), params), owner)
                        .await;
                }
                let mut result = self
                    .connection()?
                    .exec_iter(statement, params)
                    .await
                    .map_err(error)?;
                let columns = result.columns_ref().to_vec();
                let rows: Vec<Row> = result.collect().await.map_err(error)?;
                let affected = result.affected_rows();
                let insert_id = result.last_insert_id().unwrap_or(0);
                result.drop_result().await.map_err(error)?;
                self.last_insert_id = insert_id;
                Ok(RawRows::Mysql {
                    columns,
                    rows,
                    affected,
                })
            }
            Prepared::Emulated if values.is_empty() => {
                self.text(&sql.source, options.mysql_stream.as_ref()).await
            }
            Prepared::Emulated => {
                let mode = self.backslash_escapes().await?;
                let query = sql.render(|i| match &values[i] {
                    BoundValue::String(bytes) => {
                        let quoted = quote(bytes, mode);
                        if options.national_strings {
                            format!("N{quoted}")
                        } else {
                            quoted
                        }
                    }
                    value => literal(value, true),
                });
                self.text(&query, options.mysql_stream.as_ref()).await
            }
            _ => Err(DbError {
                state: "HY000".into(),
                code: None,
                detail: "Mismatched database driver".into(),
            }),
        }
    }

    pub async fn run_unprepared(&mut self, sql: &str) -> Result<RawRows, DbError> {
        self.text(sql, None).await
    }

    async fn text(&mut self, sql: &str, owner: Option<&Arc<()>>) -> Result<RawRows, DbError> {
        self.check_pending().await?;
        if !self.options.multiple {
            require_single_statement(self.connection()?, sql).await?;
        }
        if let Some(owner) = owner {
            return self
                .stream(stream::Query::Text(sql.to_owned()), owner)
                .await;
        }
        let mut result = self.connection()?.query_iter(sql).await.map_err(error)?;
        let columns = result.columns_ref().to_vec();
        let rows: Vec<Row> = result.collect().await.map_err(error)?;
        let affected = result.affected_rows();
        let insert_id = result.last_insert_id().unwrap_or(0);
        result.drop_result().await.map_err(error)?;
        self.last_insert_id = insert_id;
        Ok(RawRows::Mysql {
            columns,
            rows,
            affected,
        })
    }

    pub async fn exec(&mut self, sql: &str) -> Result<RawRows, DbError> {
        let rows = self.run_unprepared(sql).await?;
        // PHP exec() has no statement on which to consume a SELECT's result.
        self.pending_exec_rows = rows.has_columns();
        Ok(rows)
    }

    async fn check_pending(&mut self) -> Result<(), DbError> {
        if self.active.as_ref().is_some_and(stream::Active::abandoned) {
            self.finish_stream().await;
        }
        if !self.pending_exec_rows && self.active.is_none() {
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
        self.check_pending().await?;
        let mode: Option<String> = self
            .connection()?
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

    pub fn transaction_state(&self) -> Option<bool> {
        // Like mysqlnd, retain PDO's last observed status after an error packet.
        self.connection.as_ref()?.last_ok_packet().map(|packet| {
            packet
                .status_flags()
                .contains(mysql_async::consts::StatusFlags::SERVER_STATUS_IN_TRANS)
        })
    }

    pub fn last_id(&self) -> String {
        self.last_insert_id.to_string()
    }

    fn closed() -> DbError {
        DbError {
            state: "HY000".into(),
            code: Some(2006),
            detail: "MySQL server has gone away".into(),
        }
    }

    fn connection(&mut self) -> Result<&mut Conn, DbError> {
        self.connection.as_mut().ok_or_else(Self::closed)
    }
}

async fn configure(connection: &mut Conn, settings: &MysqlOptions) -> Result<(), DbError> {
    if settings.ignore_space {
        connection
            .query_drop("SET sql_mode = CONCAT_WS(',', NULLIF(@@sql_mode, ''), 'IGNORE_SPACE')")
            .await
            .map_err(error)?;
    }
    if let Some(init) = &settings.init {
        if !settings.multiple {
            require_single_statement(connection, init).await?;
        }
        connection.query_drop(init).await.map_err(error)?;
    }
    if !settings.autocommit {
        connection
            .query_drop("SET autocommit=0")
            .await
            .map_err(error)?;
    }
    Ok(())
}

/// The client library always advertises multi-statements. A fresh server parse
/// through the prepared protocol allows at most one statement before a text query.
async fn require_single_statement(connection: &mut Conn, sql: &str) -> Result<(), DbError> {
    let statement = connection.prep(sql).await.map_err(error)?;
    connection.close(statement).await.map_err(error)
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
