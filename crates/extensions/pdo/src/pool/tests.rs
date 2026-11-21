//! Live pool tests against the two dedicated database containers. They log in
//! as `phera_pool`, an account no other test uses, so the connection counts
//! the server reports belong to these tests alone.

mod isolation;
mod limits;
mod threads;

use crate::dsn::ParseDsn;

use std::sync::Arc;
use std::time::Duration;

use phera_extension_api::Value;
use phera_io_pool::Options;

use super::{Lease, Policy, Pool, acquire, acquire_from};
use crate::drivers::{Prepared, Rows};
use crate::dsn::Dsn;
use crate::errors::DbError;
use crate::placeholders::{Sql, Syntax};

const USER: &str = "phera_pool";

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
        io_threads: 2,
    }
}

/// An independent pool with small limits.
#[derive(Clone)]
struct TestPool {
    name: &'static str,
    pool: Arc<Pool>,
}

impl TestPool {
    fn new(name: &'static str, options: Options) -> Self {
        Self {
            name,
            pool: Arc::new(Pool::with_options(options)),
        }
    }

    async fn try_connect(&self) -> Result<Lease, DbError> {
        acquire_from(&self.pool, &dsn(self.name), USER, USER, Policy::Pooled).await
    }

    async fn connect(&self) -> Lease {
        self.try_connect().await.unwrap()
    }
}

/// A connection outside every pool, to watch or kill the pooled ones.
async fn direct(name: &str) -> Lease {
    acquire(&dsn(name), USER, USER, Policy::Unpooled, false)
        .await
        .unwrap()
}

async fn rows(driver: &mut Lease, sql: &str) -> Rows {
    let sql = Sql::parse(sql.into(), Syntax::POSTGRES).unwrap();
    driver
        .execute(&sql, &Prepared::Emulated, &[], Default::default())
        .await
        .unwrap()
}

async fn scalar(driver: &mut Lease, sql: &str) -> Value {
    rows(driver, sql).await.values.get(0).unwrap()[0].clone()
}

/// The server's id for the session, which tells whether two leases got the
/// same connection.
async fn id(driver: &mut Lease, name: &str) -> i64 {
    let sql = match name {
        "pgsql" => "SELECT pg_backend_pid()",
        _ => "SELECT CONNECTION_ID()",
    };
    let Value::Int(id) = scalar(driver, sql).await else {
        panic!("integer connection id")
    };
    id
}

#[tokio::test(flavor = "current_thread")]
async fn process_pool_live_contract() {
    for name in ["pgsql", "mysql"] {
        // These run before the tests of the process pool, so that the server's
        // connection counts are not mixed with its connections.
        threads::bounded_workers(name, 1, 4).await;
        threads::bounded_workers(name, 8, 2).await;
        threads::idle_handoff(name).await;
        limits::fifo_timeout_cancellation(name).await;
        limits::interrupted_socket_is_retired(name).await;
        limits::expiry_and_killed(name).await;
        let pool = TestPool::new(name, options(1));
        let mut first = pool.connect().await;
        let original = id(&mut first, name).await;
        isolation::dirty(&mut first, name).await;
        drop(first);
        let mut second = pool.connect().await;
        assert_eq!(id(&mut second, name).await, original, "{name}: reuse");
        isolation::clean(&mut second, name).await;
        isolation::reset_repeatedly(&mut second).await;
        drop(second);
        drop(pool);
        if name == "pgsql" {
            isolation::custom_types_retire_client().await;
        }
        threads::slow_query_yields_to_another_connection(name).await;
        let mut other_dsn = dsn(name);
        other_dsn.fields.insert("host".into(), "localhost".into());
        let mut first = acquire(&dsn(name), USER, USER, Policy::Pooled, false)
            .await
            .unwrap();
        let mut other = acquire(&other_dsn, USER, USER, Policy::Pooled, false)
            .await
            .unwrap();
        assert_ne!(id(&mut first, name).await, id(&mut other, name).await);
        assert!(
            acquire(&dsn(name), USER, "incorrect", Policy::Pooled, false)
                .await
                .is_err()
        );
        eprintln!(
            "{name}: reuse, isolation, keys, worker-local sockets, FIFO, expiry, kill passed"
        );
    }
}
