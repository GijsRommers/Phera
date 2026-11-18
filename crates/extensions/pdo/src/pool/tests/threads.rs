//! Each simulated PHP worker owns a current-thread runtime and local PHP values.

use std::collections::HashSet;
use std::time::Duration;

use phera_extension_api::Value;
use tokio::sync::oneshot;

use super::{TestPool, USER, direct, dsn, id, options, rows, scalar};
use crate::pool::{Key, POOL, Policy, acquire};

#[tokio::main(flavor = "current_thread")]
async fn worker(pool: Option<TestPool>, name: &'static str) -> i64 {
    let mut connection = match pool {
        Some(pool) => pool.connect().await,
        None => acquire(&dsn(name), USER, USER, Policy::Pooled)
            .await
            .unwrap(),
    };
    let identity = id(&mut connection, name).await;
    let query = if name == "pgsql" {
        "SELECT pg_sleep(0.05)"
    } else {
        "SELECT SLEEP(0.05)"
    };
    rows(&mut connection, query).await;
    identity
}

fn launch(pool: Option<TestPool>, name: &'static str) -> oneshot::Receiver<i64> {
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        // Send only after the runtime is destroyed: sockets cannot rely on it.
        let result = worker(pool, name);
        let _ = sender.send(result);
    });
    receiver
}

pub async fn shared_registry(name: &'static str) {
    let first = launch(None, name).await.unwrap();
    // The connection is returned in the background. Wait until it is back,
    // rather than have the second worker open a socket during the reset.
    POOL.settled(&Key::new(&dsn(name), USER, USER)).await;
    let second = launch(None, name).await.unwrap();
    assert_eq!(
        first, second,
        "{name}: two independent workers share one server socket"
    );
}

pub async fn bounded_users(name: &'static str) {
    let mut settings = options(3);
    settings.wait = Duration::from_secs(3);
    settings.idle = Duration::from_secs(5);
    let pool = TestPool::new(name, settings);
    let mut observer = direct(name).await;
    let query = if name == "pgsql" {
        "SELECT count(*) FROM pg_stat_activity WHERE usename='phera_pool' AND pid<>pg_backend_pid()"
    } else {
        "SELECT count(*) FROM information_schema.PROCESSLIST \
         WHERE USER='phera_pool' AND ID<>CONNECTION_ID()"
    };
    let mut workers: Vec<_> = (0..13).map(|_| launch(Some(pool.clone()), name)).collect();
    let mut identities = HashSet::new();
    let mut peak = 0;
    while !workers.is_empty() {
        let Value::Int(count) = scalar(&mut observer, query).await else {
            panic!("count")
        };
        peak = peak.max(count);
        assert!(
            count <= 3,
            "{name}: server saw {count} connections, pool limit 3"
        );
        workers.retain_mut(|worker| match worker.try_recv() {
            Ok(identity) => {
                identities.insert(identity);
                false
            }
            Err(oneshot::error::TryRecvError::Empty) => true,
            Err(error) => panic!("worker failed: {error}"),
        });
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(identities.len(), 3);
    assert_eq!(peak, 3);
    drop(pool);
    // Wait for the test-only pool's asynchronous Drop cleanup before other tests.
    for _ in 0..100 {
        if scalar(&mut observer, query).await == Value::Int(0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(scalar(&mut observer, query).await, Value::Int(0));
    eprintln!("{name}: 13 users on 13 worker runtimes, maximum 3, server peak {peak}");
}
