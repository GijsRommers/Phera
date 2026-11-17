//! PDO objects and retained statements borrow the same exclusive pool lease.

use std::rc::Rc;
use std::time::Duration;

use phera_extension_api::{ArrayKey, Object, PhpArray, Value};

mod native_support;
mod support;
use native_support::{Harness, object};

fn options(enabled: bool) -> PhpArray {
    let mut options = PhpArray::new();
    for (key, value) in [(100_000, Value::Bool(enabled)), (12, Value::Bool(true))] {
        options.insert(ArrayKey::Int(key), value);
    }
    options
}

async fn identity(harness: &mut Harness, pdo: &Rc<Object>, driver: &str) -> Value {
    let sql = if driver == "pgsql" {
        "SELECT pg_backend_pid()"
    } else {
        "SELECT CONNECTION_ID()"
    };
    let statement = object(
        harness
            .call(pdo, "query", vec![Value::from(sql)])
            .await
            .unwrap(),
    );
    harness
        .call(&statement, "fetchColumn", vec![])
        .await
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn pdo_and_statement_lifetimes_and_cancelled_request() {
    assert!(support::databases_available());
    assert!(std::path::Path::new(support::SCRATCH).is_dir());
    tokio::task::LocalSet::new()
        .run_until(async {
            for driver in ["pgsql", "mysql"] {
                let mut harness = Harness::new();

                let pdo = harness
                    .connect_options(driver, options(true))
                    .await
                    .unwrap();
                let original = identity(&mut harness, &pdo, driver).await;
                let statement = object(
                    harness
                        .call(&pdo, "query", vec![Value::from("SELECT 1")])
                        .await
                        .unwrap(),
                );
                drop(pdo);
                let other = harness.connect(driver).await;
                assert_ne!(identity(&mut harness, &other, driver).await, original);
                assert_eq!(
                    harness
                        .call(&statement, "fetchColumn", vec![])
                        .await
                        .unwrap(),
                    Value::Int(1)
                );
                drop(statement);
                tokio::time::sleep(Duration::from_millis(50)).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
                let pdo = harness
                    .connect_options(driver, options(true))
                    .await
                    .unwrap();
                assert_eq!(identity(&mut harness, &pdo, driver).await, original);
                drop(pdo);
                tokio::time::sleep(Duration::from_millis(50)).await;

                // Hold the other connection so reuse remains deterministic.
                // Cancellation during a transaction must release and reset the lease.
                let request = async {
                    let pdo = harness
                        .connect_options(driver, options(true))
                        .await
                        .unwrap();
                    harness
                        .call(&pdo, "beginTransaction", vec![])
                        .await
                        .unwrap();
                    harness
                        .call(
                            &pdo,
                            "exec",
                            vec![Value::from(
                                "CREATE TEMPORARY TABLE cancelled_request(id INT)",
                            )],
                        )
                        .await
                        .unwrap();
                    std::future::pending::<()>().await;
                };
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), request)
                        .await
                        .is_err()
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
                let pdo = harness
                    .connect_options(driver, options(true))
                    .await
                    .unwrap();
                assert_eq!(identity(&mut harness, &pdo, driver).await, original);
                assert_eq!(
                    harness.call(&pdo, "inTransaction", vec![]).await.unwrap(),
                    Value::Bool(false)
                );
                assert!(
                    harness
                        .call(
                            &pdo,
                            "query",
                            vec![Value::from("SELECT * FROM cancelled_request")]
                        )
                        .await
                        .is_err()
                );
                drop(pdo);

                drop(other);
                let first = harness
                    .connect_options(driver, options(false))
                    .await
                    .unwrap();
                let first_id = identity(&mut harness, &first, driver).await;
                drop(first);
                let second = harness
                    .connect_options(driver, options(false))
                    .await
                    .unwrap();
                assert_ne!(identity(&mut harness, &second, driver).await, first_id);
            }
        })
        .await;
}
