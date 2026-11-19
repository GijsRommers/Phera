//! Async database leases. For example, pg_pconnect() can reuse an idle session.

use std::collections::BTreeMap;
use std::time::Duration;

use phera_io_pool::{AcquireError, IoRuntime, Lease, Options, Pool};

use crate::connect_options::ConnectOptions;
use crate::drivers::{Driver, connection_error};
use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;

pub static RUNTIME: IoRuntime = IoRuntime::new("phera-db-io");

pub static POOL: Pool<Key, Driver> = Pool::new(
    &RUNTIME,
    "PHERA_PDO",
    Options {
        size: 20,
        idle: Duration::from_secs(60),
        lifetime: Duration::from_secs(1800),
        wait: Duration::from_secs(30),
        io_threads: 2,
    },
);

/// Connections are only shared between `new PDO` calls that would get the
/// same session: the same driver, settings and credentials. The password is
/// part of the key and is never printed, which is why there is no `Debug`.
#[derive(PartialEq, Eq, Hash)]
pub struct Key {
    driver: DriverName,
    fields: BTreeMap<String, String>,
    options: ConnectOptions,
    username: String,
    password: String,
}

impl Key {
    /// Credentials that came in the DSN are already the arguments, and the
    /// driver's default host, port and database are spelled out, so that
    /// equal settings written differently share one pool.
    pub fn new(dsn: &Dsn, username: &str, password: &str) -> Self {
        let mut fields = dsn.fields.clone();
        fields.remove("user");
        fields.remove("password");
        fields
            .entry("host".into())
            .or_insert_with(|| "localhost".into());
        fields
            .entry("port".into())
            .or_insert_with(|| dsn.default_port().to_string());
        fields
            .entry("dbname".into())
            .or_insert_with(|| match dsn.driver {
                DriverName::Postgres => username.to_owned(),
                // SQLite never reaches the pool.
                DriverName::Mysql | DriverName::Sqlite => String::new(),
            });
        Self {
            driver: dsn.driver,
            fields,
            options: dsn.options.clone(),
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }
}

/// Whether a database connection borrows from the pool.
#[derive(Clone, Copy)]
pub enum Policy {
    Pooled,
    Unpooled,
}

pub async fn acquire(
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease<Driver>, DbError> {
    acquire_from(&POOL, dsn, username, password, policy).await
}

pub async fn acquire_from(
    pool: &Pool<Key, Driver>,
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease<Driver>, DbError> {
    let connect = {
        let (dsn, username, password) = (dsn.clone(), username.to_owned(), password.to_owned());
        async move { Driver::connect(&dsn, &username, &password).await }
    };
    // A SQLite handle is never pooled: see `drivers::sqlite`.
    let policy = match dsn.driver {
        DriverName::Sqlite => Policy::Unpooled,
        _ => policy,
    };
    let lease = match policy {
        Policy::Pooled => {
            pool.acquire(Key::new(dsn, username, password), connect)
                .await
        }
        Policy::Unpooled => pool.connect_unpooled(connect).await,
    };
    lease.map_err(|error| match error {
        AcquireError::Settings(detail) => DbError::general(detail),
        AcquireError::Runtime(detail) => connection_error(detail),
        AcquireError::Exhausted { limit } => DbError::general(format!(
            "PDO connection pool exhausted (limit {limit}); timed out waiting"
        )),
        AcquireError::Connect(error) => error,
    })
}
