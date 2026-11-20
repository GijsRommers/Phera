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

use phera_extension_api::value::to_bool;
use phera_extension_api::{ArrayKey, PhpArray};

pub use phera_db_drivers::pool::Policy;

#[cfg(test)]
use crate::drivers::Driver;
#[cfg(test)]
use phera_db_drivers::pool::{Key, POOL, RUNTIME};
#[cfg(test)]
use phera_io_pool::Pool;

use crate::dsn::Dsn;
use crate::errors::DbError;

/// `PDO::PHERA_ATTR_POOL`, Phera's own constructor option.
pub const PHERA_ATTR_POOL: i64 = 100_000;

pub trait FromOptions {
    fn from_options(options: Option<&PhpArray>) -> Self;
}

impl FromOptions for Policy {
    /// Pooled unless the constructor options say `PDO::PHERA_ATTR_POOL => false`.
    fn from_options(options: Option<&PhpArray>) -> Self {
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
    Lease::new(phera_db_drivers::pool::acquire(dsn, username, password, policy).await?).await
}

#[cfg(test)]
async fn acquire_from(
    pool: &Pool<Key, Driver>,
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease, DbError> {
    Lease::new(phera_db_drivers::pool::acquire_from(pool, dsn, username, password, policy).await?)
        .await
}

#[cfg(test)]
mod tests;
