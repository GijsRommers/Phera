//! The pool's contract, with a connection that only counts what happens to it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::{AcquireError, Connection, IoRuntime, Options, Pool};

static RUNTIME: IoRuntime = IoRuntime::new("phera-io-pool-test");

#[derive(Default)]
struct Counts {
    opened: AtomicUsize,
    resets: AtomicUsize,
    closed: AtomicUsize,
}

struct Fake {
    id: usize,
    counts: Arc<Counts>,
    healthy: bool,
    reusable: bool,
}

impl Connection for Fake {
    type Error = String;

    async fn check(&mut self, _idle: Duration) -> bool {
        self.healthy
    }

    fn reusable(&self) -> bool {
        self.reusable
    }

    async fn reset(&mut self) -> Result<(), String> {
        self.counts.resets.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn close(self) {
        self.counts.closed.fetch_add(1, Ordering::SeqCst);
    }
}

fn pool(size: usize) -> Pool<&'static str, Fake> {
    Pool::with_options(
        &RUNTIME,
        Options {
            size,
            idle: Duration::from_secs(60),
            lifetime: Duration::from_secs(60),
            wait: Duration::from_millis(100),
            io_threads: 1,
        },
    )
}

fn open(counts: &Arc<Counts>) -> impl Future<Output = Result<Fake, String>> + Send + 'static {
    let counts = counts.clone();
    async move {
        let id = counts.opened.fetch_add(1, Ordering::SeqCst);
        Ok(Fake {
            id,
            counts,
            healthy: true,
            reusable: true,
        })
    }
}

async fn id(lease: &super::Lease<Fake>) -> usize {
    lease.run(|fake| Box::pin(async move { fake.id })).await.unwrap()
}

async fn update(lease: &super::Lease<Fake>, change: fn(&mut Fake)) {
    lease
        .run(move |fake| Box::pin(async move { change(fake) }))
        .await
        .unwrap();
}

/// Waits until a returned connection has been reset and put back.
async fn returned(pool: &Pool<&'static str, Fake>) {
    pool.settled(&"key").await;
}

#[tokio::test]
async fn a_returned_connection_is_reset_and_lent_again() {
    let (pool, counts) = (pool(1), Arc::new(Counts::default()));
    let first = pool.acquire("key", open(&counts)).await.unwrap();
    let original = id(&first).await;
    drop(first);
    returned(&pool).await;
    let second = pool.acquire("key", open(&counts)).await.unwrap();
    assert_eq!(id(&second).await, original);
    assert_eq!(counts.opened.load(Ordering::SeqCst), 1);
    assert_eq!(counts.resets.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failing_checks_and_unreusable_connections_are_closed() {
    let (pool, counts) = (pool(1), Arc::new(Counts::default()));
    let first = pool.acquire("key", open(&counts)).await.unwrap();
    update(&first, |fake| fake.healthy = false).await;
    drop(first);
    returned(&pool).await;
    let second = pool.acquire("key", open(&counts)).await.unwrap();
    assert_eq!(id(&second).await, 1, "the idle one failed its check");
    update(&second, |fake| fake.reusable = false).await;
    drop(second);
    returned(&pool).await;
    assert_eq!(counts.closed.load(Ordering::SeqCst), 2);
    assert_eq!(counts.resets.load(Ordering::SeqCst), 1, "no reset when unreusable");
}

#[tokio::test]
async fn a_full_pool_times_out_and_a_cancelled_waiter_leaves_the_queue() {
    let (pool, counts) = (pool(1), Arc::new(Counts::default()));
    let first = pool.acquire("key", open(&counts)).await.unwrap();
    assert!(matches!(
        pool.acquire("key", open(&counts)).await,
        Err(AcquireError::Exhausted { limit: 1 })
    ));
    let cancelled = tokio::time::timeout(
        Duration::from_millis(10),
        pool.acquire("key", open(&counts)),
    );
    assert!(cancelled.await.is_err());
    drop(first);
    let next = pool.acquire("key", open(&counts)).await.unwrap();
    assert_eq!(id(&next).await, 0, "the waiter that gave up took nothing");
    assert_eq!(counts.opened.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_unpooled_connection_is_closed_not_reset() {
    let (pool, counts) = (pool(1), Arc::new(Counts::default()));
    let lease = pool.connect_unpooled(open(&counts)).await.unwrap();
    let pooled = pool.acquire("key", open(&counts)).await.unwrap();
    drop((lease, pooled));
    returned(&pool).await;
    for _ in 0..100 {
        if counts.closed.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(counts.closed.load(Ordering::SeqCst), 1);
    assert_eq!(counts.resets.load(Ordering::SeqCst), 1);
}
