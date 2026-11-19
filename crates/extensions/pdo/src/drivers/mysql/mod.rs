//! MySQL uses binary prepared statements, or text queries for `ATTR_EMULATE_PREPARES`.

use std::time::Duration;

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder, Params, Row, Statement};

mod information;
mod tls;
pub mod values;

use super::{Prepared, RawRows};
use crate::connect_options::MysqlOptions;
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::parameters::{BoundValue, literal, quote};
use crate::placeholders::Sql;
use values::parameter;

pub struct Mysql {
    connection: Conn,
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
            connection,
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

    pub async fn close(self) {
        let _ = self.connection.disconnect().await;
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        if !self.connection.reset().await.map_err(error)? {
            self.connection
                .change_user(Default::default())
                .await
                .map_err(error)?;
        }
        // COM_RESET_CONNECTION does not restore the database selected with USE.
        if !self.database.is_empty() {
            let database = self.database.replace('`', "``");
            self.connection
                .query_drop(format!("USE `{database}`"))
                .await
                .map_err(error)?;
        }
        if let Some(charset) = &self.charset {
            self.connection
                .query_drop(format!("SET NAMES {charset}"))
                .await
                .map_err(error)?;
        }
        configure(&mut self.connection, &self.options).await?;
        self.last_insert_id = 0;
        self.pending_exec_rows = false;
        Ok(())
    }

    pub fn is_broken(&self) -> bool {
        self.connection.is_disconnected()
    }

    pub async fn health(&mut self) -> Result<(), DbError> {
        self.connection.ping().await.map_err(error)
    }

    pub async fn set_autocommit(&mut self, enabled: bool) -> Result<(), DbError> {
        self.check_pending()?;
        self.connection
            .query_drop(if enabled {
                "SET autocommit=1"
            } else {
                "SET autocommit=0"
            })
            .await
            .map_err(error)
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
    ) -> Result<RawRows, DbError> {
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
                Ok(RawRows::Mysql {
                    columns,
                    rows,
                    affected,
                })
            }
            Prepared::Emulated if values.is_empty() => self.run_unprepared(&sql.source).await,
            Prepared::Emulated => {
                let mode = self.backslash_escapes().await?;
                let query = sql.render(|i| match &values[i] {
                    BoundValue::String(bytes) => quote(bytes, mode),
                    value => literal(value, true),
                });
                self.run_unprepared(&query).await
            }
            _ => Err(DbError {
                state: "HY000".into(),
                code: None,
                detail: "Mismatched database driver".into(),
            }),
        }
    }

    pub async fn run_unprepared(&mut self, sql: &str) -> Result<RawRows, DbError> {
        self.check_pending()?;
        let mut result = self.connection.query_iter(sql).await.map_err(error)?;
        let columns = result.columns_ref().to_vec();
        let rows: Vec<Row> = result.collect().await.map_err(error)?;
        let affected = result.affected_rows();
        self.last_insert_id = result.last_insert_id().unwrap_or(0);
        result.drop_result().await.map_err(error)?;
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

    pub fn transaction_state(&self) -> Option<bool> {
        // Like mysqlnd, retain PDO's last observed status after an error packet.
        self.connection.last_ok_packet().map(|packet| {
            packet
                .status_flags()
                .contains(mysql_async::consts::StatusFlags::SERVER_STATUS_IN_TRANS)
        })
    }

    pub fn last_id(&self) -> String {
        self.last_insert_id.to_string()
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
