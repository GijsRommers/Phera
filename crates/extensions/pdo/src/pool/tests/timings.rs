//! Paired samples isolate a driver query from PHP conversion and alternate order.

use std::time::Instant;

use super::{direct, runtime};
use crate::pool::Lease;

async fn query_batch(connection: &Lease, dispatched: bool) -> f64 {
    let shared = connection.shared.clone();
    let started = Instant::now();
    if dispatched {
        for _ in 0..300 {
            let shared = shared.clone();
            runtime::run(async move {
                shared
                    .driver
                    .lock()
                    .await
                    .as_mut()
                    .unwrap()
                    .direct("SELECT 1")
                    .await
            })
            .await
            .unwrap();
        }
        started.elapsed().as_secs_f64() * 1_000_000.0 / 300.0
    } else {
        runtime::run(async move {
            let mut guard = shared.driver.lock().await;
            let driver = guard.as_mut().unwrap();
            let started = Instant::now();
            for _ in 0..300 {
                driver.direct("SELECT 1").await?;
            }
            Ok(started.elapsed().as_secs_f64() * 1_000_000.0 / 300.0)
        })
        .await
        .unwrap()
    }
}

pub async fn measure(name: &'static str) {
    let connection = direct(name).await;
    let mut deltas = Vec::new();
    for sample in 0..6 {
        let (direct, dispatched) = if sample % 2 == 0 {
            let direct = query_batch(&connection, false).await;
            (direct, query_batch(&connection, true).await)
        } else {
            let dispatched = query_batch(&connection, true).await;
            (query_batch(&connection, false).await, dispatched)
        };
        deltas.push(dispatched - direct);
        eprintln!("{name}: sample {sample}: driver {direct:.3} us; dispatched {dispatched:.3} us");
    }
    deltas.sort_by(f64::total_cmp);
    let median = (deltas[2] + deltas[3]) / 2.0;
    eprintln!("{name}: query crossing delta median {median:.3} us; samples {deltas:?}");
    let started = Instant::now();
    for _ in 0..1000 {
        runtime::run(async { Ok(()) }).await.unwrap();
    }
    eprintln!(
        "{name}: empty worker/I/O round trip {:.3} us",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let shared = connection.shared.clone();
    let reset = runtime::run(async move {
        let mut guard = shared.driver.lock().await;
        let driver = guard.as_mut().unwrap();
        let started = Instant::now();
        for _ in 0..300 {
            driver.reset().await?;
        }
        Ok(started.elapsed())
    })
    .await
    .unwrap();
    eprintln!(
        "{name}: reset on I/O runtime mean {:.3} us (300 samples)",
        reset.as_secs_f64() * 1_000_000.0 / 300.0
    );
}
