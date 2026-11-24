//! MySQL connection options configured before returning a PDO connection.

use std::time::Duration;

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, OptsBuilder};

use super::{Mysql, configure, error, tls};
use crate::dsn::Dsn;
use crate::errors::DbError;

impl Mysql {
    pub async fn connect(dsn: &Dsn, user: &str, password: &str) -> Result<Self, DbError> {
        let port = dsn.port()?;
        let settings = &dsn.options.mysql;
        let connect = async {
            let mut options = OptsBuilder::default()
                .ip_or_hostname(dsn.get("host", "localhost"))
                .tcp_port(port)
                .tcp_nodelay(true)
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
            } else {
                options = options.stmt_cache_size(32);
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
        let charset = dsn.fields.get("charset").cloned();
        let database = dsn.get("dbname", "").to_owned();
        let (reset_sql_with_db, reset_sql_keep_db) =
            super::build_reset_sql(&database, charset.as_deref(), settings);
        Ok(Self {
            connection: Some(connection),
            active: None,
            version: version.unwrap_or_default(),
            last_insert_id: 0,
            pending_exec_rows: false,
            db_changed: false,
            session_dirty: false,
            reset_sql_with_db,
            reset_sql_keep_db,
            charset,
            database,
            options: settings.clone(),
            reusable: true,
            host: dsn.get("host", "localhost").to_owned(),
            unix_socket: dsn.fields.contains_key("unix_socket"),
        })
    }
}
