//! A database connection stays on the worker that opens it.
//!
//! `new PDO($dsn, $user, $password)` can reuse one of that worker's idle connections.

mod admission;
mod lease;
mod socket;
mod worker;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use phera_io_pool::Options;
use tokio::time::timeout;

pub use lease::{BackgroundOperation, Lease, Operation};

use admission::Admission;
use socket::Socket;
use worker::Worker;

use crate::connect_options::ConnectOptions;
use crate::drivers::Driver;
use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;

pub static POOL: Pool = Pool::new();

const DEFAULTS: Options = Options {
    size: 64,
    idle: Duration::from_secs(60),
    lifetime: Duration::from_secs(1800),
    wait: Duration::from_secs(30),
    io_threads: 2,
};

/// The same server and database share a limit even when session settings differ.
#[derive(PartialEq, Eq)]
struct Database {
    driver: DriverName,
    host: String,
    port: String,
    database: String,
    socket: Option<String>,
}

/// Only sessions opened with the same settings and credentials can be reused.
/// A key contains the password and must never be printed.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    driver: DriverName,
    fields: BTreeMap<String, String>,
    options: ConnectOptions,
    username: String,
    password: String,
    policy: Policy,
}

impl Key {
    fn new(dsn: &Dsn, username: &str, password: &str, policy: Policy) -> Self {
        let mut fields = dsn.fields.clone();
        fields.remove("user");
        fields.remove("password");
        fields
            .entry("host".into())
            .or_insert_with(|| "localhost".into());
        fields
            .entry("port".into())
            .or_insert_with(|| dsn.default_port().to_string());
        fields.entry("dbname".into()).or_insert_with(|| {
            if dsn.driver == DriverName::Postgres {
                username.to_owned()
            } else {
                String::new()
            }
        });
        Self {
            driver: dsn.driver,
            fields,
            options: dsn.options.clone(),
            username: username.to_owned(),
            password: password.to_owned(),
            policy,
        }
    }

    fn database(&self) -> Database {
        let field = |name| self.fields.get(name).cloned().unwrap_or_default();
        Database {
            driver: self.driver,
            host: field("host"),
            port: field("port"),
            database: field("dbname"),
            socket: self.fields.get("unix_socket").cloned(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Pooled,
    Persistent,
    Unpooled,
}

pub struct Pool {
    options: OnceLock<Result<Options, String>>,
    databases: Mutex<Vec<(Database, Arc<Admission>)>>,
}

impl Pool {
    const fn new() -> Self {
        Self {
            options: OnceLock::new(),
            databases: Mutex::new(Vec::new()),
        }
    }

    /// Fixed limits for live pool tests that must be isolated from each other.
    pub fn with_options(options: Options) -> Self {
        Self {
            options: OnceLock::from(Ok(options)),
            databases: Mutex::new(Vec::new()),
        }
    }

    fn options(&self) -> Result<Options, DbError> {
        self.options
            .get_or_init(|| Options::pool_limits_from_environment("PHERA_PDO", DEFAULTS))
            .as_ref()
            .copied()
            .map_err(DbError::general)
    }

    fn admission(&self, database: Database, size: usize) -> Arc<Admission> {
        let mut databases = self
            .databases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, admission)) = databases.iter().find(|(key, _)| *key == database) {
            return admission.clone();
        }
        let admission = Arc::new(Admission::new(size));
        databases.push((database, admission.clone()));
        admission
    }

    pub async fn acquire(
        &self,
        dsn: &Dsn,
        username: &str,
        password: &str,
        policy: Policy,
    ) -> Result<Lease, DbError> {
        let options = self.options()?;
        let connect = Driver::connect(dsn, username, password);
        if dsn.driver == DriverName::Sqlite || policy == Policy::Unpooled {
            return Ok(Lease::new(Socket::new(connect.await?, None), None));
        }
        let key = Key::new(dsn, username, password, policy);
        let worker = Worker::current(self.admission(key.database(), options.size), options);
        timeout(options.wait, worker.lend(Arc::new(key), connect))
            .await
            .map_err(|_| {
                DbError::general(format!(
                    "PDO connection pool exhausted (limit {}); timed out waiting",
                    options.size
                ))
            })?
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        let databases = self
            .databases
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (_, admission) in databases {
            admission.shutdown();
        }
    }
}

pub async fn acquire(
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease, DbError> {
    POOL.acquire(dsn, username, password, policy).await
}
