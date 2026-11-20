//! What a PDO object can do with its borrowed connection. Each method sends
//! owned data to the I/O runtime and converts the driver's answer back into
//! PHP values here, on the worker.

use std::rc::Rc;

use phera_db_drivers::session_log::SessionLog;
use phera_extension_api::PhpArray;
use phera_io_pool::Operation;

use crate::constants::ATTR_SERVER_INFO;
use crate::drivers::{Column, Driver, Execution, Prepared, Rows, connection_error};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;
use std::sync::Arc;

pub struct Lease {
    connection: Link,
    /// `PDO::ATTR_SERVER_VERSION`, read once at connect time.
    version: String,
    /// The transaction state the server reported after the last operation,
    /// for drivers that report it with every reply (MySQL).
    observed_transaction: Option<bool>,
    /// How the connection was made, so that a fork can make it again.
    origin: Rc<Origin>,
    /// What the script changed about the session, for a fork to replay.
    log: SessionLog,
}

/// The borrowed connection, or in a fork of a snapshot, the settings to
/// borrow one with on first use (see [`Lease::forked`]).
enum Link {
    Open(phera_io_pool::Lease<Driver>),
    Reconnect,
}

/// What `new PDO` was given: enough to connect again.
pub struct Origin {
    pub dsn: crate::dsn::Dsn,
    pub username: String,
    pub password: String,
    pub policy: super::Policy,
}

impl Lease {
    pub(super) async fn new(
        connection: phera_io_pool::Lease<Driver>,
        origin: Origin,
    ) -> Result<Self, DbError> {
        let inspect = |driver: &Driver| {
            (
                driver.version().to_owned(),
                driver.observed_transaction_state(),
            )
        };
        // Nothing else uses a lease that was just lent, so this reads it on
        // the worker at once; were it ever busy, the I/O runtime is asked.
        let (version, observed_transaction) = match connection.try_inspect(inspect) {
            Some(found) => found,
            None => connection
                .run(move |driver| Box::pin(async move { inspect(driver) }))
                .await
                .map_err(connection_error)?,
        };
        Ok(Self {
            connection: Link::Open(connection),
            version,
            observed_transaction,
            origin: Rc::new(origin),
            log: SessionLog::default(),
        })
    }

    /// A copy for a fork of a snapshot: it borrows its own session on first
    /// use, with the same settings, and replays what the script changed
    /// about this one (see `phera_db_drivers::session_log`). The pool
    /// resets a session before it lends it again, so forks share nothing.
    ///
    /// ```php
    /// $pdo = new PDO('mysql:host=db;dbname=shop', 'app', 'secret');
    /// $pdo->exec("SET time_zone = '+00:00'");
    /// Phera\snapshot();
    /// $pdo->query('SELECT NOW()');   // each request: its own session, UTC
    /// ```
    pub fn forked(&self) -> Result<Self, String> {
        const ADVICE: &str = "; finish it before Phera\\snapshot(), or connect after the call";
        if let Some(reason) = self.log.unreplayable() {
            return Err(format!(
                "{reason}, which a new session would not have{ADVICE}"
            ));
        }
        if self.observed_transaction == Some(true) {
            return Err(format!("a transaction is open on it{ADVICE}"));
        }
        if let Link::Open(connection) = &self.connection {
            let memory = connection
                .try_inspect(|driver| matches!(driver, Driver::Sqlite(_)))
                .ok_or_else(|| format!("it is busy with an operation{ADVICE}"))?
                && sqlite_in_memory(&self.origin.dsn);
            if memory {
                return Err(format!(
                    "an in-memory SQLite database lives only in its connection{ADVICE}"
                ));
            }
        }
        Ok(Self {
            connection: Link::Reconnect,
            version: self.version.clone(),
            observed_transaction: None,
            origin: self.origin.clone(),
            log: self.log.clone(),
        })
    }

    /// The borrowed connection; a fork's copy borrows it now.
    async fn connection(&mut self) -> Result<&phera_io_pool::Lease<Driver>, DbError> {
        if let Link::Reconnect = self.connection {
            let origin = &self.origin;
            let lease = phera_db_drivers::pool::acquire(
                &origin.dsn,
                &origin.username,
                &origin.password,
                origin.policy,
            )
            .await?;
            let statements = self.log.statements().to_vec();
            lease
                .run(move |driver| {
                    Box::pin(async move {
                        for statement in statements {
                            driver
                                .run_unprepared(&String::from_utf8_lossy(&statement))
                                .await?;
                        }
                        Ok(())
                    })
                })
                .await
                .map_err(connection_error)??;
            self.connection = Link::Open(lease);
        }
        match &self.connection {
            Link::Open(lease) => Ok(lease),
            Link::Reconnect => Err(connection_error("the connection could not be made again")),
        }
    }

    /// Notes a statement that succeeded, for a fork to replay.
    fn note(&mut self, sql: &str) {
        if SessionLog::concerns(sql.as_bytes()) {
            self.log.ran(sql.as_bytes());
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Runs one driver operation on the I/O runtime, and remembers the
    /// transaction state the server reported with it.
    async fn run<T, F>(&mut self, operation: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut Driver) -> Operation<'a, Result<T, DbError>> + Send + 'static,
    {
        let (result, observed) = self
            .connection()
            .await?
            .run(|driver| {
                Box::pin(async move {
                    let result = operation(driver).await;
                    (result, driver.observed_transaction_state())
                })
            })
            .await
            .map_err(connection_error)?;
        self.observed_transaction = observed;
        result
    }

    /// See [`Driver::prepare`] for `number`.
    pub async fn prepare(&mut self, sql: &Sql, number: u64) -> Result<Prepared, DbError> {
        let sql = sql.clone();
        self.run(move |driver| Box::pin(async move { driver.prepare(&sql, number).await }))
            .await
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: Execution,
    ) -> Result<Rows, DbError> {
        let source = values.is_empty().then(|| sql.source.clone());
        let (sql, prepared, values) = (sql.clone(), prepared.clone(), values.to_vec());
        let rows = self
            .run(move |driver| {
                Box::pin(async move { driver.execute(&sql, &prepared, &values, &options).await })
            })
            .await?;
        if let Some(source) = source {
            self.note(&source);
        }
        rows.into_php()
    }

    pub async fn read_stream(
        &mut self,
        owner: Arc<()>,
        all: bool,
    ) -> Result<Option<(Rows, bool)>, DbError> {
        let result = self
            .run(move |driver| {
                Box::pin(async move {
                    match driver {
                        Driver::Mysql(mysql) => mysql.read_stream(&owner, all).await,
                        Driver::Postgres(postgres) => postgres.read_lazy(&owner, all).await,
                        _ => Ok(None),
                    }
                })
            })
            .await?;
        result
            .map(|(rows, done)| rows.into_php().map(|rows| (rows, done)))
            .transpose()
    }

    pub async fn close_stream(&mut self, owner: Arc<()>) -> Result<(), DbError> {
        self.run(move |driver| {
            Box::pin(async move {
                match driver {
                    Driver::Mysql(mysql) => mysql.close_stream(&owner).await,
                    Driver::Postgres(postgres) => postgres.close_lazy(&owner),
                    Driver::Sqlite(_) => {}
                }
                Ok(())
            })
        })
        .await
    }

    pub async fn exec(&mut self, sql: &str) -> Result<Rows, DbError> {
        let owned = sql.to_owned();
        let rows = self
            .run(move |driver| Box::pin(async move { driver.exec(&owned).await }))
            .await?;
        self.note(sql);
        rows.into_php()
    }

    pub async fn run_unprepared(&mut self, sql: &str) -> Result<Rows, DbError> {
        let owned = sql.to_owned();
        let rows = self
            .run(move |driver| Box::pin(async move { driver.run_unprepared(&owned).await }))
            .await?;
        self.note(sql);
        rows.into_php()
    }

    pub async fn last_id(&mut self, name: Option<&str>) -> Result<String, DbError> {
        let name = name.map(str::to_owned);
        self.run(move |driver| Box::pin(async move { driver.last_id(name.as_deref()).await }))
            .await
    }

    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        let bytes = bytes.to_vec();
        self.run(move |driver| Box::pin(async move { driver.quote(&bytes).await }))
            .await
    }

    /// `getColumnMeta()`. Only PostgreSQL needs to ask the server.
    pub async fn metadata(&mut self, column: &Column) -> Result<PhpArray, DbError> {
        let postgres = match &column.metadata {
            Metadata::Mysql(mysql) => return Ok(crate::metadata::mysql(mysql)),
            Metadata::Sqlite(sqlite, kind) => {
                return Ok(crate::drivers::sqlite::statement::metadata(sqlite, *kind));
            }
            Metadata::Postgres(postgres) => postgres,
        };
        let owned = column.clone();
        let (length, table) = self
            .run(move |driver| Box::pin(async move { driver.postgres_column(&owned).await }))
            .await?;
        Ok(crate::metadata::postgres_values(
            &column.name,
            postgres,
            length,
            table,
        ))
    }

    /// `ATTR_TIMEOUT` on a SQLite connection.
    pub async fn sqlite_timeout(&mut self, milliseconds: u64) -> Result<(), DbError> {
        self.run(move |driver| {
            Box::pin(async move {
                match driver {
                    Driver::Sqlite(sqlite) => sqlite.set_timeout(milliseconds).await,
                    _ => Err(connection_error("ATTR_TIMEOUT is set on SQLite only")),
                }
            })
        })
        .await?;
        self.log
            .record(format!("PRAGMA busy_timeout = {milliseconds}").into_bytes());
        Ok(())
    }

    pub async fn mysql_autocommit(&mut self, enabled: bool) -> Result<(), DbError> {
        self.run(move |driver| {
            Box::pin(async move {
                if let Driver::Mysql(mysql) = driver {
                    mysql.set_autocommit(enabled).await
                } else {
                    Err(connection_error("ATTR_AUTOCOMMIT is set on MySQL only"))
                }
            })
        })
        .await?;
        self.log
            .record(format!("SET autocommit={}", u8::from(enabled)).into_bytes());
        Ok(())
    }

    pub async fn server_attribute(&mut self, attribute: i64) -> Result<String, DbError> {
        self.run(move |driver| {
            Box::pin(async move {
                match (driver, attribute) {
                    (Driver::Mysql(mysql), ATTR_SERVER_INFO) => mysql.server_info().await,
                    (Driver::Postgres(postgres), ATTR_SERVER_INFO) => {
                        postgres.buffer_lazy().await;
                        postgres.server_info().await
                    }
                    (Driver::Mysql(mysql), _) => Ok(mysql.connection_status()),
                    (Driver::Postgres(postgres), _) => Ok(postgres.connection_status().into()),
                    (Driver::Sqlite(_), _) => Err(connection_error("Missing server connection")),
                }
            })
        })
        .await
    }

    pub fn observed_transaction_state(&self) -> Option<bool> {
        self.observed_transaction
    }

    pub async fn transaction_state(&mut self) -> Option<bool> {
        self.run(|driver| Box::pin(async move { Ok(driver.transaction_state().await) }))
            .await
            .ok()
            .flatten()
    }

    #[cfg(test)]
    pub async fn reset(&mut self) -> Result<(), DbError> {
        self.run(|driver| Box::pin(async move { driver.reset().await }))
            .await
    }
}

/// Whether the DSN names SQLite's in-memory database (`sqlite::memory:`,
/// or an empty path, which is a temporary database of the connection).
fn sqlite_in_memory(dsn: &crate::dsn::Dsn) -> bool {
    let path = dsn.get("filename", "");
    path.is_empty() || path == ":memory:" || path.contains("mode=memory")
}
