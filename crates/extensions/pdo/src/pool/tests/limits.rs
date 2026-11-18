//! FIFO admission, dropped futures, idle/lifetime retirement and dead sockets.

use std::time::Duration;

use tokio::task::LocalSet;

use super::{TestPool, direct, id, options, rows};

pub async fn fifo_timeout_cancellation(name: &'static str) {
    LocalSet::new()
        .run_until(async move {
            let pool = TestPool::new(name, options(1));
            let first = pool.connect().await;
            let error = pool.try_connect().await.err().unwrap();
            assert!(error.detail.contains("pool exhausted (limit 1)"));
            let mut waiters = Vec::new();
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            for index in 0..8 {
                let pool = pool.clone();
                let sender = sender.clone();
                waiters.push(tokio::task::spawn_local(async move {
                    let _connection = pool.connect().await;
                    sender.send(index).unwrap();
                }));
                // Submit and poll each waiter before the next one joins the FIFO.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            waiters.remove(3).abort();
            drop(first);
            for expected in [0, 1, 2, 4, 5, 6, 7] {
                assert_eq!(receiver.recv().await.unwrap(), expected);
            }
            for waiter in waiters {
                waiter.await.unwrap();
            }
            let mut connection = pool.connect().await;
            let original = id(&mut connection, name).await;
            drop(connection);
            // Drop a request while a query is in flight. Its operation retains the
            // lease until the driver protocol completes, then rollback/reset runs.
            let request = async {
                let mut connection = pool.connect().await;
                connection.run_unprepared("BEGIN").await.unwrap();
                let query = if name == "pgsql" {
                    "SELECT pg_sleep(0.08)"
                } else {
                    "SELECT SLEEP(0.08)"
                };
                rows(&mut connection, query).await;
            };
            assert!(
                tokio::time::timeout(Duration::from_millis(20), request)
                    .await
                    .is_err()
            );
            let mut next = pool.connect().await;
            assert_eq!(id(&mut next, name).await, original);
            next.run_unprepared("CREATE TEMPORARY TABLE after_cancel(id INT)")
                .await
                .unwrap();
            eprintln!(
                "{name}: FIFO 8 waiters, cancelled waiter and in-flight request, timeout passed"
            );
        })
        .await;
}

pub async fn expiry_and_killed(name: &'static str) {
    let pool = TestPool::new(name, options(1));
    let mut first = pool.connect().await;
    let original = id(&mut first, name).await;
    drop(first);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let mut next = pool.connect().await;
    assert_ne!(id(&mut next, name).await, original, "idle expiry");
    drop(next);
    drop(pool);

    let mut settings = options(1);
    settings.idle = Duration::from_secs(20);
    settings.lifetime = Duration::from_millis(200);
    let pool = TestPool::new(name, settings);
    let mut first = pool.connect().await;
    let original = id(&mut first, name).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    drop(first);
    let mut next = pool.connect().await;
    assert_ne!(
        id(&mut next, name).await,
        original,
        "borrowed lifetime expiry"
    );
    let idle_original = id(&mut next, name).await;
    drop(next);
    tokio::time::sleep(Duration::from_millis(450)).await;
    let mut next = pool.connect().await;
    assert_ne!(
        id(&mut next, name).await,
        idle_original,
        "idle lifetime expiry"
    );
    drop(next);
    drop(pool);

    settings.lifetime = Duration::from_secs(30);
    let pool = TestPool::new(name, settings);
    let mut first = pool.connect().await;
    let original = id(&mut first, name).await;
    drop(first);
    // Wait beyond the cheap-health-check threshold, without idle expiry.
    tokio::time::sleep(Duration::from_millis(3100)).await;
    let mut killer = direct(name).await;
    if name == "pgsql" {
        rows(
            &mut killer,
            &format!("SELECT pg_terminate_backend({original})"),
        )
        .await;
    } else {
        killer
            .run_unprepared(&format!("KILL {original}"))
            .await
            .unwrap();
    }
    let mut replacement = pool.connect().await;
    assert_ne!(
        id(&mut replacement, name).await,
        original,
        "killed idle socket replaced"
    );
    let borrowed_id = id(&mut replacement, name).await;
    if name == "pgsql" {
        rows(
            &mut killer,
            &format!("SELECT pg_terminate_backend({borrowed_id})"),
        )
        .await;
    } else {
        killer
            .run_unprepared(&format!("KILL {borrowed_id}"))
            .await
            .unwrap();
    }
    drop(replacement);
    let mut after_reset_failure = pool.connect().await;
    assert_ne!(
        id(&mut after_reset_failure, name).await,
        borrowed_id,
        "failed reset discarded socket"
    );
}
