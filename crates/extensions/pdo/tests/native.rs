//! Native API integration: `$stmt->bindParam(1, $id); $id = 2; $stmt->execute()`.

use phera_extension_api::{PhpArray, Value};
use std::cell::RefCell;
use std::rc::Rc;
mod native_support;
mod support;
use native_support::{Harness, object};

#[tokio::test(flavor = "current_thread")]
async fn both_drivers_bind_fetch_and_rollback() {
    tokio::task::LocalSet::new()
        .run_until(async {
            if !support::databases_available() {
                return;
            }
            // All test-created temporary artifacts belong to the authorized scratch directory.
            assert!(std::path::Path::new(support::SCRATCH).is_dir());
            for driver in ["pgsql", "mysql"] {
                let mut harness = Harness::new();
                let pdo = harness.connect(driver).await;
                harness
                    .call(
                        &pdo,
                        "setAttribute",
                        vec![Value::Int(20), Value::Bool(false)],
                    )
                    .await
                    .unwrap();
                harness
            .call(
                &pdo,
                "exec",
                vec![Value::from(
                    "CREATE TEMPORARY TABLE phera_native (id INT PRIMARY KEY, label VARCHAR(80))",
                )],
            )
            .await
            .unwrap();
                let insert = object(
                    harness
                        .call(
                            &pdo,
                            "prepare",
                            vec![Value::from("INSERT INTO phera_native VALUES (?, ?)")],
                        )
                        .await
                        .unwrap(),
                );
                let id = Rc::new(RefCell::new(Value::Int(1)));
                harness
                    .call(
                        &insert,
                        "bindParam",
                        vec![Value::Int(1), Value::Reference(id.clone()), Value::Int(1)],
                    )
                    .await
                    .unwrap();
                harness
                    .call(
                        &insert,
                        "bindValue",
                        vec![Value::Int(2), Value::from("O'Reilly")],
                    )
                    .await
                    .unwrap();
                *id.borrow_mut() = Value::Int(2);
                assert_eq!(
                    harness.call(&insert, "execute", vec![]).await.unwrap(),
                    Value::Bool(true)
                );
                assert_eq!(
                    harness.call(&insert, "rowCount", vec![]).await.unwrap(),
                    Value::Int(1)
                );
                let query = object(
                    harness
                        .call(
                            &pdo,
                            "query",
                            vec![Value::from("SELECT id, label FROM phera_native")],
                        )
                        .await
                        .unwrap(),
                );
                let mut expected = PhpArray::new();
                expected.push(Value::Int(2)).unwrap();
                expected.push(Value::from("O'Reilly")).unwrap();
                assert_eq!(
                    harness
                        .call(&query, "fetch", vec![Value::Int(3)])
                        .await
                        .unwrap(),
                    Value::from(expected)
                );
                harness
                    .call(&pdo, "beginTransaction", vec![])
                    .await
                    .unwrap();
                harness
                    .call(&pdo, "exec", vec![Value::from("DELETE FROM phera_native")])
                    .await
                    .unwrap();
                harness.call(&pdo, "rollBack", vec![]).await.unwrap();
                let query = object(
                    harness
                        .call(
                            &pdo,
                            "query",
                            vec![Value::from("SELECT COUNT(*) FROM phera_native")],
                        )
                        .await
                        .unwrap(),
                );
                assert_eq!(
                    harness.call(&query, "fetchColumn", vec![]).await.unwrap(),
                    Value::Int(1)
                );
            }
        })
        .await;
}
