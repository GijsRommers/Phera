//! Dropping a PDO rolls back its transaction: `$pdo = null;` releases a locked row.

use std::time::{Duration, Instant};

use phera_extension_api::Value;
use tokio::task::{JoinSet, LocalSet};

mod native_support;
mod support;
use native_support::{Harness, object};

#[tokio::test(flavor = "current_thread")]
async fn dropping_connections_rolls_back_and_releases_locks() {
    tokio::task::LocalSet::new()
        .run_until(async {
            if !support::databases_available() {
                return;
            }
            for driver in ["pgsql", "mysql"] {
                let mut harness = Harness::new();
                let first = harness.connect(driver).await;
                let second = harness.connect(driver).await;
                harness
                    .call(
                        &first,
                        "exec",
                        vec![Value::from("DROP TABLE IF EXISTS phera_native_lifecycle")],
                    )
                    .await
                    .unwrap();
                harness
                    .call(
                        &first,
                        "exec",
                        vec![Value::from(
                            "CREATE TABLE phera_native_lifecycle (id INT PRIMARY KEY)",
                        )],
                    )
                    .await
                    .unwrap();
                harness
                    .call(&first, "beginTransaction", vec![])
                    .await
                    .unwrap();
                harness
                    .call(
                        &first,
                        "exec",
                        vec![Value::from("INSERT INTO phera_native_lifecycle VALUES (1)")],
                    )
                    .await
                    .unwrap();
                drop(first);
                tokio::time::timeout(
                    Duration::from_secs(1),
                    harness.call(
                        &second,
                        "exec",
                        vec![Value::from("INSERT INTO phera_native_lifecycle VALUES (1)")],
                    ),
                )
                .await
                .expect("dropped PDO must release the row lock")
                .unwrap();
                harness
                    .call(
                        &second,
                        "exec",
                        vec![Value::from("DROP TABLE phera_native_lifecycle")],
                    )
                    .await
                    .unwrap();
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn forty_native_database_operations_share_one_thread() {
    tokio::task::LocalSet::new()
        .run_until(async {
            if !support::databases_available() {
                return;
            }
            LocalSet::new()
                .run_until(async {
                    for driver in ["pgsql", "mysql"] {
                        let started = Instant::now();
                        let mut operations = JoinSet::new();
                        for _ in 0..40 {
                            operations.spawn_local(async move {
                                let mut harness = Harness::new();
                                let pdo = harness.connect(driver).await;
                                let sql = if driver == "pgsql" {
                                    "SELECT pg_sleep(0.3)"
                                } else {
                                    "SELECT SLEEP(0.3)"
                                };
                                let statement = object(
                                    harness
                                        .call(&pdo, "query", vec![Value::from(sql)])
                                        .await
                                        .unwrap(),
                                );
                                harness.call(&statement, "fetch", vec![]).await.unwrap();
                            });
                        }
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while let Some(result) = operations.join_next().await {
                                result.unwrap();
                            }
                        })
                        .await
                        .expect("database waits finish");
                        let elapsed = started.elapsed();
                        eprintln!("{driver}: 40 native waits on one thread took {elapsed:?}");
                        // One after the other the forty waits would take 12 seconds. The
                        // limit leaves room for forty password handshakes, which are slow
                        // in an unoptimised test build.
                        assert!(elapsed < Duration::from_secs(6), "{driver}: {elapsed:?}");
                    }
                })
                .await;
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn last_insert_ids_survive_quoting_and_postgres_sequences_are_named() {
    tokio::task::LocalSet::new()
        .run_until(async {
            if !support::databases_available() {
                return;
            }
            for driver in ["pgsql", "mysql"] {
                let mut harness = Harness::new();
                let pdo = harness.connect(driver).await;
                let definition = if driver == "pgsql" {
                    "id SERIAL PRIMARY KEY"
                } else {
                    "id INT AUTO_INCREMENT PRIMARY KEY"
                };
                harness
            .call(
                &pdo,
                "exec",
                vec![Value::from(format!(
                    "CREATE TEMPORARY TABLE phera_native_ids ({definition}, label VARCHAR(30))"
                ))],
            )
            .await
            .unwrap();
                for emulate in [false, true] {
                    harness
                        .call(
                            &pdo,
                            "setAttribute",
                            vec![Value::Int(20), Value::Bool(emulate)],
                        )
                        .await
                        .unwrap();
                    let statement = object(
                        harness
                            .call(
                                &pdo,
                                "prepare",
                                vec![Value::from(
                                    "INSERT INTO phera_native_ids (label) VALUES ('hello')",
                                )],
                            )
                            .await
                            .unwrap(),
                    );
                    harness.call(&statement, "execute", vec![]).await.unwrap();
                    harness
                        .call(&pdo, "quote", vec![Value::from("O'Reilly")])
                        .await
                        .unwrap();
                    let expected = Value::from(if emulate { "2" } else { "1" });
                    assert_eq!(
                        harness.call(&pdo, "lastInsertId", vec![]).await.unwrap(),
                        expected
                    );
                    if driver == "pgsql" {
                        assert_eq!(
                            harness
                                .call(
                                    &pdo,
                                    "lastInsertId",
                                    vec![Value::from("phera_native_ids_id_seq")]
                                )
                                .await
                                .unwrap(),
                            expected
                        );
                    }
                }
            }
        })
        .await;
}
