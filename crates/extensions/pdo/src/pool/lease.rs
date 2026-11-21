//! A PDO borrows its worker's connection for each operation, including `$pdo->query($sql)`.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use phera_db_drivers::pool::Lease as DriverLease;
use phera_db_drivers::session_log::SessionLog;
use phera_extension_api::PhpArray;

use crate::constants::ATTR_SERVER_INFO;
use crate::drivers::{Column, Driver, Execution, Prepared, Rows, connection_error};
use crate::errors::DbError;
use crate::metadata::Metadata;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

pub struct Lease {
    connection: OnceCell<DriverLease>,
    boot: Option<Boot>,
    replay: Option<Rc<Replay>>,
    version: String,
    observed_transaction: Cell<Option<bool>>,
}

pub struct Origin {
    pub dsn: crate::dsn::Dsn,
    pub username: String,
    pub password: String,
    pub policy: super::Policy,
}

struct Boot {
    origin: Rc<Origin>,
    log: RefCell<SessionLog>,
}

struct Replay {
    origin: Rc<Origin>,
    statements: Vec<Vec<u8>>,
}

impl Lease {
    pub(super) fn new(connection: DriverLease, origin: Option<Origin>) -> Result<Self, DbError> {
        let (version, observed_transaction) = connection
            .try_inspect(|driver| {
                (
                    driver.version().to_owned(),
                    driver.observed_transaction_state(),
                )
            })
            .ok_or_else(|| connection_error("Missing server connection"))?;
        Ok(Self {
            connection: OnceCell::from(connection),
            boot: origin.map(|origin| Boot {
                origin: Rc::new(origin),
                log: RefCell::new(SessionLog::new(true)),
            }),
            replay: None,
            version,
            observed_transaction: Cell::new(observed_transaction),
        })
    }

    /// The snapshot stores the connection settings and the boot's session log.
    /// Each fork borrows a worker-local socket on its first operation.
    pub fn forked(&self) -> Result<Self, String> {
        const ADVICE: &str = "; finish it before Phera\\snapshot(), or connect after the call";
        let replay = if let Some(boot) = &self.boot {
            let log = boot.log.borrow();
            if let Some(reason) = log.unreplayable() {
                return Err(format!(
                    "{reason}, which a new session would not have{ADVICE}"
                ));
            }
            Rc::new(Replay {
                origin: boot.origin.clone(),
                statements: log.statements().to_vec(),
            })
        } else {
            self.replay
                .clone()
                .ok_or("it was opened in a way a fork cannot repeat")?
        };
        if self.observed_transaction.get() == Some(true) {
            return Err(format!("a transaction is open on it{ADVICE}"));
        }
        if let Some(connection) = self.connection.get() {
            let sqlite = connection
                .try_inspect(|driver| matches!(driver, Driver::Sqlite(_)))
                .ok_or_else(|| format!("it is busy with an operation{ADVICE}"))?;
            if sqlite && sqlite_in_memory(&replay.origin.dsn) {
                return Err(format!(
                    "an in-memory SQLite database lives only in its connection{ADVICE}"
                ));
            }
        }
        Ok(Self {
            connection: OnceCell::new(),
            boot: None,
            replay: Some(replay),
            version: self.version.clone(),
            observed_transaction: Cell::new(None),
        })
    }

    async fn reconnect(&self) -> Result<&DriverLease, DbError> {
        let replay = self
            .replay
            .as_ref()
            .ok_or_else(|| connection_error("Missing server connection"))?;
        let origin = &replay.origin;
        let lease = phera_db_drivers::pool::acquire(
            &origin.dsn,
            &origin.username,
            &origin.password,
            origin.policy,
        )
        .await?;
        lease
            .run(async |driver| {
                for statement in &replay.statements {
                    driver
                        .run_unprepared(&String::from_utf8_lossy(statement))
                        .await?;
                }
                Ok(())
            })
            .await?;
        Ok(self.connection.get_or_init(|| lease))
    }

    fn note(&self, sql: &str) {
        if let Some(boot) = &self.boot {
            boot.log.borrow_mut().ran(sql.as_bytes());
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    async fn run<T>(
        &self,
        operation: impl AsyncFnOnce(&mut Driver) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let connection = match self.connection.get() {
            Some(connection) => connection,
            None => self.reconnect().await?,
        };
        connection
            .run(async move |driver| {
                let result = operation(driver).await;
                self.observed_transaction
                    .set(driver.observed_transaction_state());
                result
            })
            .await
    }

    pub async fn prepare(&self, sql: &Sql, number: u64) -> Result<Prepared, DbError> {
        self.run(async |driver| driver.prepare(sql, number).await)
            .await
    }

    pub async fn execute(
        &self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: Execution,
    ) -> Result<Rows, DbError> {
        let rows = self
            .run(async |driver| driver.execute(sql, prepared, values, &options).await)
            .await?;
        if values.is_empty() {
            self.note(&sql.source);
        }
        Ok(rows)
    }

    pub async fn read_stream(
        &self,
        owner: Arc<()>,
        all: bool,
    ) -> Result<Option<(Rows, bool)>, DbError> {
        let result = self
            .run(async |driver| match driver {
                Driver::Mysql(mysql) => mysql.read_stream(&owner, all).await,
                Driver::Postgres(postgres) => postgres.read_lazy(&owner, all).await,
                Driver::Sqlite(_) => Ok(None),
            })
            .await?;
        result
            .map(|(rows, done)| rows.into_php().map(|rows| (rows, done)))
            .transpose()
    }

    pub async fn close_stream(&self, owner: Arc<()>) -> Result<(), DbError> {
        self.run(async |driver| {
            match driver {
                Driver::Mysql(mysql) => mysql.close_stream(&owner).await,
                Driver::Postgres(postgres) => postgres.close_lazy(&owner),
                Driver::Sqlite(_) => {}
            }
            Ok(())
        })
        .await
    }

    pub async fn exec(&self, sql: &str) -> Result<Rows, DbError> {
        let rows = self.run(async |driver| driver.exec(sql).await).await?;
        self.note(sql);
        Ok(rows)
    }

    pub async fn run_unprepared(&self, sql: &str) -> Result<Rows, DbError> {
        let rows = self
            .run(async |driver| driver.run_unprepared(sql).await)
            .await?;
        self.note(sql);
        Ok(rows)
    }

    pub async fn last_id(&self, name: Option<&str>) -> Result<String, DbError> {
        self.run(async |driver| driver.last_id(name).await).await
    }

    pub async fn quote(&self, bytes: &[u8]) -> Result<String, DbError> {
        self.run(async |driver| driver.quote(bytes).await).await
    }

    /// `getColumnMeta()`. Only PostgreSQL needs to ask the server.
    pub async fn metadata(&self, column: &Column) -> Result<PhpArray, DbError> {
        let postgres = match &column.metadata {
            Metadata::Mysql(mysql) => return Ok(crate::metadata::mysql(mysql)),
            Metadata::Sqlite(sqlite, kind) => {
                return Ok(crate::drivers::sqlite::statement::metadata(sqlite, *kind));
            }
            Metadata::Postgres(postgres) => postgres,
        };
        let (length, table) = self
            .run(async |driver| driver.postgres_column(column).await)
            .await?;
        Ok(crate::metadata::postgres_values(
            &column.name,
            postgres,
            length,
            table,
        ))
    }

    /// `ATTR_TIMEOUT` on a SQLite connection.
    pub async fn sqlite_timeout(&self, milliseconds: u64) -> Result<(), DbError> {
        self.run(async |driver| match driver {
            Driver::Sqlite(sqlite) => sqlite.set_timeout(milliseconds).await,
            _ => Err(connection_error("ATTR_TIMEOUT is set on SQLite only")),
        })
        .await?;
        if let Some(boot) = &self.boot {
            boot.log
                .borrow_mut()
                .record(format!("PRAGMA busy_timeout = {milliseconds}").into_bytes());
        }
        Ok(())
    }

    pub async fn mysql_autocommit(&self, enabled: bool) -> Result<(), DbError> {
        self.run(async |driver| match driver {
            Driver::Mysql(mysql) => mysql.set_autocommit(enabled).await,
            _ => Err(connection_error("ATTR_AUTOCOMMIT is set on MySQL only")),
        })
        .await?;
        if let Some(boot) = &self.boot {
            boot.log
                .borrow_mut()
                .record(format!("SET autocommit={}", u8::from(enabled)).into_bytes());
        }
        Ok(())
    }

    pub async fn server_attribute(&self, attribute: i64) -> Result<String, DbError> {
        self.run(async |driver| match (driver, attribute) {
            (Driver::Mysql(mysql), ATTR_SERVER_INFO) => mysql.server_info().await,
            (Driver::Postgres(postgres), ATTR_SERVER_INFO) => {
                postgres.buffer_lazy().await;
                postgres.server_info().await
            }
            (Driver::Mysql(mysql), _) => Ok(mysql.connection_status()),
            (Driver::Postgres(postgres), _) => Ok(postgres.connection_status().into()),
            (Driver::Sqlite(_), _) => Err(connection_error("Missing server connection")),
        })
        .await
    }

    pub fn observed_transaction_state(&self) -> Option<bool> {
        self.observed_transaction.get()
    }

    pub async fn transaction_state(&self) -> Option<bool> {
        self.run(async |driver| Ok(driver.transaction_state().await))
            .await
            .ok()
            .flatten()
    }

    #[cfg(test)]
    pub async fn reset(&self) -> Result<(), DbError> {
        self.run(async |driver| driver.reset().await).await
    }
}

/// An in-memory or temporary SQLite database lives only in its connection.
fn sqlite_in_memory(dsn: &crate::dsn::Dsn) -> bool {
    let path = dsn.get("filename", "");
    path.is_empty() || path == ":memory:" || path.contains("mode=memory")
}
