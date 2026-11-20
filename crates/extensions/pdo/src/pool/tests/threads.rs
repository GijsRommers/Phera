//! Each simulated PHP worker keeps its own runtime alive between requests.

use std::time::Duration;

use phera_extension_api::Value;
use tokio::sync::oneshot;
use tokio::task::{JoinSet, LocalSet};
use tokio::time::{sleep, timeout};

use super::{TestPool, direct, id, options, rows, scalar};
use crate::pool::Lease;

pub async fn socket_count(observer: &mut Lease, name: &str) -> i64 {
    let query = if name == "pgsql" {
        "SELECT count(*) FROM pg_stat_activity WHERE usename='phera_pool' AND pid<>pg_backend_pid()"
    } else {
        "SELECT count(*) FROM information_schema.PROCESSLIST \
         WHERE USER='phera_pool' AND ID<>CONNECTION_ID()"
    };
    let Value::Int(count) = scalar(observer, query).await else {
        panic!("expected connection count");
    };
    count
}

pub async fn wait_for_socket_count(observer: &mut Lease, name: &str, expected: i64) {
    timeout(Duration::from_secs(3), async {
        while socket_count(observer, name).await != expected {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn load(pool: TestPool, requests: usize) {
    LocalSet::new()
        .run_until(async {
            let mut tasks = JoinSet::new();
            for _ in 0..requests {
                let pool = pool.clone();
                tasks.spawn_local(async move {
                    let mut connection = pool.connect().await;
                    let query = if pool.name == "pgsql" {
                        "SELECT pg_sleep(0.04)"
                    } else {
                        "SELECT SLEEP(0.04)"
                    };
                    rows(&mut connection, query).await;
                });
            }
            while let Some(task) = tasks.join_next().await {
                task.unwrap();
            }
            // Leave time for the observer to see idle sockets as well.
            sleep(Duration::from_millis(50)).await;
        })
        .await;
}

pub async fn bounded_workers(name: &'static str, limit: i64, workers: usize) {
    let mut settings = options(limit as usize);
    settings.wait = Duration::from_secs(5);
    settings.idle = Duration::from_secs(30);
    let pool = TestPool::new(name, settings);
    let mut observer = direct(name).await;
    let mut active: Vec<_> = (0..workers)
        .map(|_| {
            let pool = pool.clone();
            std::thread::spawn(move || load(pool, 8))
        })
        .collect();
    let mut peak = 0;
    while !active.is_empty() {
        let seen = socket_count(&mut observer, name).await;
        peak = peak.max(seen);
        assert!(
            seen <= limit,
            "{name}: server saw {seen} connections, limit {limit}"
        );
        let mut index = 0;
        while index < active.len() {
            if active[index].is_finished() {
                active.swap_remove(index).join().unwrap();
            } else {
                index += 1;
            }
        }
        sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(peak, limit, "{name}: peak connection count");
    wait_for_socket_count(&mut observer, name, 0).await;
    eprintln!("{name}: {workers} workers finished all queries, server peak {peak}");
}

#[tokio::main(flavor = "current_thread")]
async fn idle_worker(pool: TestPool, ready: oneshot::Sender<i64>, finish: oneshot::Receiver<()>) {
    let mut first = pool.connect().await;
    let identity = id(&mut first, pool.name).await;
    drop(first);
    ready.send(identity).unwrap();
    finish.await.unwrap();
}

pub async fn idle_handoff(name: &'static str) {
    let mut settings = options(1);
    settings.idle = Duration::from_secs(30);
    let pool = TestPool::new(name, settings);
    let (ready, idle) = oneshot::channel();
    let (finish, finished) = oneshot::channel();
    let worker = {
        let pool = pool.clone();
        std::thread::spawn(move || idle_worker(pool, ready, finished))
    };
    let old = idle.await.unwrap();
    let mut observer = direct(name).await;
    assert_eq!(socket_count(&mut observer, name).await, 1);
    let mut fresh = timeout(Duration::from_secs(1), pool.connect())
        .await
        .expect("idle worker releases the slot before idle expiry");
    assert_ne!(id(&mut fresh, name).await, old);
    assert_eq!(socket_count(&mut observer, name).await, 1);
    finish.send(()).unwrap();
    worker.join().unwrap();
    drop(fresh);
    drop(pool);
    wait_for_socket_count(&mut observer, name, 0).await;
}

pub async fn slow_query_yields_to_another_connection(name: &'static str) {
    let pool = TestPool::new(name, options(2));
    let mut slow = pool.connect().await;
    let mut fast = pool.connect().await;
    let query = if name == "pgsql" {
        "SELECT pg_sleep(0.8)"
    } else {
        "SELECT SLEEP(0.8)"
    };
    let slow_query = rows(&mut slow, query);
    let fast_query = async {
        sleep(Duration::from_millis(25)).await;
        assert_eq!(
            timeout(Duration::from_millis(500), scalar(&mut fast, "SELECT 42"))
                .await
                .unwrap(),
            Value::Int(42)
        );
    };
    tokio::join!(slow_query, fast_query);
}
