//! A script still writes `$pdo = new PDO($dsn); $pdo->query('SELECT 1');`.
//! Its PDO and statements share one exclusive lease from a process-wide pool.
//! Dropping their final handle schedules rollback/reset in the background.
//!
//! ```php
//! $pdo = new PDO($dsn, $user, $password); // pooled automatically
//! $direct = new PDO($dsn, $user, $password, [PDO::PHERA_ATTR_POOL => false]);
//! ```
//!
//! ```text
//! PHP worker                  dedicated Tokio runtime (phera-db-io)
//! SQL + owned parameters ---> connect / execute / reset / close
//! await JoinHandle       <--- driver rows (Send)
//! convert rows to PHP values
//! ```
//!
//! Application settings are read once from the environment:
//! PHERA_PDO_POOL_SIZE=20, PHERA_PDO_POOL_IDLE_SECONDS=60,
//! PHERA_PDO_POOL_LIFETIME_SECONDS=1800, PHERA_PDO_POOL_WAIT_SECONDS=30,
//! PHERA_PDO_IO_THREADS=2. Limits apply per session/credential key across all workers.
//! ATTR_PERSISTENT is accepted and has no effect. Scripts cannot change pool limits.

mod operations;
mod options;
mod runtime;
mod state;

pub use options::{Options, Policy, is_pool_attribute};

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{Mutex, OwnedSemaphorePermit, oneshot};
use tokio::time::{Instant, timeout};

use crate::drivers::{Driver, connection_error};
use crate::dsn::{DriverName, Dsn};
use crate::errors::DbError;
use state::Bucket;

#[derive(PartialEq, Eq, Hash)]
struct Key {
    driver: DriverName,
    fields: BTreeMap<String, String>,
    username: String,
    password: String,
}

static POOLS: OnceLock<Mutex<HashMap<Key, Arc<Bucket>>>> = OnceLock::new();

struct ReturnTo {
    bucket: Arc<Bucket>,
    permit: OwnedSemaphorePermit,
    born: Instant,
}

struct SharedLease {
    driver: Mutex<Option<Driver>>,
    return_to: Option<ReturnTo>,
    runtime: Handle,
}

impl Drop for SharedLease {
    fn drop(&mut self) {
        let Some(driver) = self.driver.get_mut().take() else {
            return;
        };
        let return_to = self.return_to.take();
        self.runtime.spawn(async move {
            match return_to {
                Some(owner) => owner.bucket.recycle(driver, owner.born, owner.permit).await,
                None => driver.close().await,
            }
        });
    }
}

pub struct Lease {
    shared: Arc<SharedLease>,
    version: String,
    observed_transaction: Option<bool>,
    #[cfg(test)]
    name: DriverName,
}

impl Lease {
    fn new(driver: Driver, return_to: Option<ReturnTo>) -> Self {
        Self {
            version: driver.version().to_owned(),
            observed_transaction: driver.observed_transaction_state(),
            #[cfg(test)]
            name: match &driver {
                Driver::Postgres(_) => DriverName::Postgres,
                Driver::Mysql(_) => DriverName::Mysql,
                Driver::Sqlite(_) => DriverName::Sqlite,
            },
            shared: Arc::new(SharedLease {
                driver: Mutex::new(Some(driver)),
                return_to,
                runtime: Handle::current(),
            }),
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

pub async fn acquire(
    dsn: &Dsn,
    username: &str,
    password: &str,
    policy: Policy,
) -> Result<Lease, DbError> {
    // Neither memory nor file databases can be reset completely for a new PHP object.
    let policy = if dsn.driver == DriverName::Sqlite {
        Policy::Disabled
    } else {
        policy
    };
    let options = options::application()?;
    let (dsn, username, password) = (dsn.clone(), username.to_owned(), password.to_owned());
    let handle = runtime::handle().await?;
    let (sender, receiver) = oneshot::channel();
    handle.spawn(async move {
        let bucket = match policy {
            Policy::Disabled => None,
            Policy::Enabled => {
                // Key the effective connection settings, not redundant DSN credentials
                // or an omitted spelling of a driver's default host/port/database.
                let mut fields = dsn.fields.clone();
                fields.remove("user");
                fields.remove("password");
                fields
                    .entry("host".into())
                    .or_insert_with(|| "localhost".into());
                fields
                    .entry("port".into())
                    .or_insert_with(|| match dsn.driver {
                        DriverName::Postgres => "5432".into(),
                        DriverName::Mysql => "3306".into(),
                        DriverName::Sqlite => String::new(),
                    });
                fields
                    .entry("dbname".into())
                    .or_insert_with(|| match dsn.driver {
                        DriverName::Postgres => username.clone(),
                        DriverName::Mysql | DriverName::Sqlite => String::new(),
                    });
                let key = Key {
                    driver: dsn.driver,
                    fields,
                    username: username.clone(),
                    password: password.clone(),
                };
                let mut pools = POOLS.get_or_init(Mutex::default).lock().await;
                Some(
                    pools
                        .entry(key)
                        .or_insert_with(|| Bucket::new(options))
                        .clone(),
                )
            }
        };
        deliver(sender, bucket, dsn, username, password).await;
    });
    match policy {
        Policy::Disabled => receiver.await.map_err(connection_error)?,
        Policy::Enabled => timeout(options.wait, receiver)
            .await
            .map_err(|_| exhausted(options.size))?
            .map_err(connection_error)?,
    }
}

async fn deliver(
    mut sender: oneshot::Sender<Result<Lease, DbError>>,
    bucket: Option<Arc<Bucket>>,
    dsn: Dsn,
    username: String,
    password: String,
) {
    let result = match bucket {
        None => Driver::connect(&dsn, &username, &password)
            .await
            .map(|driver| Lease::new(driver, None)),
        Some(bucket) => {
            // Cancelling a queued request removes its FIFO waiter immediately.
            let permit = tokio::select! {
                _ = sender.closed() => return,
                permit = bucket.capacity.clone().acquire_owned() => permit,
            };
            match permit {
                Ok(permit) => borrow(bucket, permit, &dsn, &username, &password).await,
                Err(error) => Err(connection_error(error)),
            }
        }
    };
    // If cancelled during I/O, dropping the undelivered lease still resets it.
    let _ = sender.send(result);
}

async fn borrow(
    bucket: Arc<Bucket>,
    permit: OwnedSemaphorePermit,
    dsn: &Dsn,
    username: &str,
    password: &str,
) -> Result<Lease, DbError> {
    while let Some(mut idle) = bucket.take_idle().await {
        let stale = idle.returned.elapsed() >= Duration::from_secs(3);
        let healthy = !stale
            || matches!(
                timeout(Duration::from_secs(5), idle.driver.health()).await,
                Ok(Ok(()))
            );
        if bucket.expired(&idle) || idle.driver.is_broken() || !healthy {
            idle.driver.close().await;
            continue;
        }
        return Ok(Lease::new(
            idle.driver,
            Some(ReturnTo {
                bucket,
                permit,
                born: idle.born,
            }),
        ));
    }
    let driver = Driver::connect(dsn, username, password).await?;
    Ok(Lease::new(
        driver,
        Some(ReturnTo {
            bucket,
            permit,
            born: Instant::now(),
        }),
    ))
}

fn exhausted(limit: usize) -> DbError {
    DbError {
        state: "HY000".into(),
        code: None,
        detail: format!("PDO connection pool exhausted (limit {limit}); timed out waiting"),
    }
}

#[cfg(test)]
mod tests;
