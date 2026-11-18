//! PDO connections come from a process-wide pool, shared by every worker:
//!
//! ```php
//! $pdo = new PDO($dsn, $user, $password);          // borrows a pooled connection
//! $direct = new PDO($dsn, $user, $password, [
//!     PDO::PHERA_ATTR_POOL => false,               // its own connection, closed at the end
//! ]);
//! ```
//!
//! A PDO object and its statements share one lease. After the last of them is
//! gone, the connection is reset in the background (the driver's `reset`)
//! and lent to the next `new PDO` with the same settings. How the pool works,
//! and how a query travels to the I/O threads and back, is described in the
//! `phera-io-pool` crate. The limits are environment settings, read once:
//! `PHERA_PDO_POOL_SIZE` (20), `PHERA_PDO_POOL_IDLE_SECONDS` (60),
//! `PHERA_PDO_POOL_LIFETIME_SECONDS` (1800), `PHERA_PDO_POOL_WAIT_SECONDS`
//! (30) and `PHERA_PDO_IO_THREADS` (2). `PDO::ATTR_PERSISTENT` is accepted
//! and changes nothing.

mod lease;

pub use lease::Lease;

use std::collections::BTreeMap;
use std::time::Duration;

use phera_extension_api::value::to_bool;
use phera_extension_api::{ArrayKey, PhpArray};
use phera_io_pool::{AcquireError, IoRuntime, Options, Pool};

use crate::drivers::{Driver, connection_error};
use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;

static RUNTIME: IoRuntime = IoRuntime::new("phera-db-io");

static POOL: Pool<Key, Driver> = Pool::new(
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
struct Key {
    driver: DriverName,
    fields: BTreeMap<String, String>,
    username: String,
    password: String,
}

impl Key {
    /// Credentials that came in the DSN are already the arguments, and the
    /// driver's default host, port and database are spelled out, so that
    /// equal settings written differently share one pool.
    fn new(dsn: &Dsn, username: &str, password: &str) -> Self {
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
                DriverName::Mysql => String::new(),
            });
        Self {
            driver: dsn.driver,
            fields,
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }
}

/// `PDO::PHERA_ATTR_POOL`, Phera's own constructor option.
pub const PHERA_ATTR_POOL: i64 = 100_000;

/// Whether a `new PDO` borrows from the pool.
#[derive(Clone, Copy)]
pub enum Policy {
    Pooled,
    Unpooled,
}

impl Policy {
    /// Pooled unless the constructor options say `PDO::PHERA_ATTR_POOL => false`.
    pub fn from_options(options: Option<&PhpArray>) -> Self {
        let opted_out = options
            .and_then(|options| options.get(&ArrayKey::Int(PHERA_ATTR_POOL)))
            .is_some_and(|value| !to_bool(value));
        if opted_out {
            Self::Unpooled
        } else {
            Self::Pooled
        }
    }
}

pub async fn acquire(
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease, DbError> {
    acquire_from(&POOL, dsn, username, password, policy).await
}

async fn acquire_from(
    pool: &Pool<Key, Driver>,
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease, DbError> {
    let connect = {
        let (dsn, username, password) = (dsn.clone(), username.to_owned(), password.to_owned());
        async move { Driver::connect(&dsn, &username, &password).await }
    };
    let lease = match policy {
        Policy::Pooled => {
            pool.acquire(Key::new(dsn, username, password), connect)
                .await
        }
        Policy::Unpooled => pool.connect_unpooled(connect).await,
    };
    let lease = lease.map_err(|error| match error {
        AcquireError::Settings(detail) => DbError::general(detail),
        AcquireError::Runtime(detail) => connection_error(detail),
        AcquireError::Exhausted { limit } => DbError::general(format!(
            "PDO connection pool exhausted (limit {limit}); timed out waiting"
        )),
        AcquireError::Connect(error) => error,
    })?;
    Lease::new(lease).await
}

#[cfg(test)]
mod tests;
