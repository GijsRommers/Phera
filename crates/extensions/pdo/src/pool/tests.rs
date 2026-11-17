//! Live pool tests use only the two dedicated database containers.

mod isolation;
mod limits;
mod threads;
mod timings;

use std::sync::Arc;
use std::time::Duration;

use phera_extension_api::Value;
use tokio::sync::oneshot;
use tokio::time::timeout;

use super::{Lease, Options, Policy, acquire, deliver, exhausted, runtime, state::Bucket};
use crate::drivers::{Prepared, Rows};
use crate::dsn::{DriverName, Dsn};
use crate::placeholders::Sql;

fn dsn(driver: &str) -> Dsn {
    let port = if driver == "pgsql" { 54329 } else { 33069 };
    Dsn::parse(&format!(
        "{driver}:host=127.0.0.1;port={port};dbname=phera_test"
    ))
    .unwrap()
}

fn options(size: usize) -> Options {
    Options {
        size,
        idle: Duration::from_millis(150),
        lifetime: Duration::from_secs(10),
        wait: Duration::from_secs(2),
        ..Options::default()
    }
}

#[derive(Clone)]
struct TestPool {
    name: &'static str,
    bucket: Arc<Bucket>,
}

impl TestPool {
    async fn new(name: &'static str, options: Options) -> Self {
        let bucket = runtime::run(async move { Ok(Bucket::new(options)) })
            .await
            .unwrap();
        Self { name, bucket }
    }

    async fn try_connect(&self) -> Result<Lease, crate::errors::DbError> {
        let (sender, receiver) = oneshot::channel();
        let bucket = self.bucket.clone();
        let dsn = dsn(self.name);
        runtime::handle().await.unwrap().spawn(deliver(
            sender,
            Some(bucket),
            dsn,
            "phera_pool".into(),
            "phera_pool".into(),
        ));
        timeout(self.bucket.options.wait, receiver)
            .await
            .map_err(|_| exhausted(self.bucket.options.size))?
            .unwrap()
    }

    async fn connect(&self) -> Lease {
        self.try_connect().await.unwrap()
    }
}

async fn direct(name: &str) -> Lease {
    acquire(&dsn(name), "phera_pool", "phera_pool", Policy::Disabled)
        .await
        .unwrap()
}

async fn rows(driver: &mut Lease, sql: &str) -> Rows {
    driver
        .execute(&Sql::parse(sql.into()).unwrap(), &Prepared::Emulated, &[])
        .await
        .unwrap()
}

async fn scalar(driver: &mut Lease, sql: &str) -> Value {
    rows(driver, sql).await.values[0][0].clone()
}

async fn id(driver: &mut Lease) -> i64 {
    let sql = match driver.name {
        DriverName::Postgres => "SELECT pg_backend_pid()",
        DriverName::Mysql => "SELECT CONNECTION_ID()",
    };
    let Value::Int(id) = scalar(driver, sql).await else {
        panic!("integer connection id")
    };
    id
}

#[tokio::test(flavor = "current_thread")]
async fn process_pool_live_contract() {
    for name in ["pgsql", "mysql"] {
        // These run before application-pool tests, so observer counts are isolated.
        threads::bounded_users(name).await;
        limits::fifo_timeout_cancellation(name).await;
        limits::expiry_and_killed(name).await;
        let pool = TestPool::new(name, options(1)).await;
        let mut first = pool.connect().await;
        let original = id(&mut first).await;
        isolation::dirty(&mut first, name).await;
        drop(first);
        let mut second = pool.connect().await;
        assert_eq!(id(&mut second).await, original, "{name}: reuse");
        isolation::clean(&mut second, name).await;
        isolation::reset_cost(&mut second, name).await;
        drop(second);
        drop(pool);
        if name == "pgsql" {
            isolation::custom_types_retire_client().await;
        }
        timings::measure(name).await;
        threads::shared_registry(name).await;
        let mut other_dsn = dsn(name);
        other_dsn.fields.insert("host".into(), "localhost".into());
        let mut first = acquire(&dsn(name), "phera_pool", "phera_pool", Policy::Enabled)
            .await
            .unwrap();
        let mut other = acquire(&other_dsn, "phera_pool", "phera_pool", Policy::Enabled)
            .await
            .unwrap();
        assert_ne!(id(&mut first).await, id(&mut other).await);
        assert!(
            acquire(&dsn(name), "phera_pool", "incorrect", Policy::Enabled)
                .await
                .is_err()
        );
        eprintln!(
            "{name}: reuse, isolation, keys, cross-worker sharing, FIFO, expiry, kill passed"
        );
    }
}
