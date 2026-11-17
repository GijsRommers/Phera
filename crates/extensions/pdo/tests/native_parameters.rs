//! Bound values and escaping: `$stmt->bindValue(':flag', false, PDO::PARAM_BOOL)`.

use std::process::Command;

use phera_extension_api::Value;
use phera_extension_api::testing::oracle::{PHP_PREAMBLE, encode, php_literal};

mod native_support;
mod support;
use native_support::{Harness, object};

#[tokio::test(flavor = "current_thread")]
async fn bindings_quoting_and_grouping_match_php() {
    tokio::task::LocalSet::new()
        .run_until(async {
            if !support::databases_available() {
                return;
            }
            for driver in ["pgsql", "mysql"] {
                let mut harness = Harness::new();
                let pdo = harness.connect(driver).await;
                let mut php = format!(
                    "{PHP_PREAMBLE}\n$p=new PDO('{}','phera','phera');\n",
                    support::dsn(driver)
                );
                let mut actual = Vec::new();
                for emulate in [false, true] {
                    harness
                        .call(
                            &pdo,
                            "setAttribute",
                            vec![Value::Int(20), Value::Bool(emulate)],
                        )
                        .await
                        .unwrap();
                    php.push_str(&format!("$p->setAttribute(20,{});\n", i32::from(emulate)));
                    let query = if driver == "pgsql" {
                        "SELECT :i::int AS i, :s::text AS s, :b::bool AS b, :n::text AS n"
                    } else {
                        "SELECT CAST(:i AS SIGNED) AS i, :s AS s, :b AS b, :n AS n"
                    };
                    let statement = object(
                        harness
                            .call(&pdo, "prepare", vec![Value::from(query)])
                            .await
                            .unwrap(),
                    );
                    php.push_str(&format!(
                        "$s=$p->prepare({});\n",
                        php_literal(&Value::from(query))
                    ));
                    for (name, value, kind) in [
                        (":i", Value::from("42"), 1),
                        (":s", Value::from("é😀 O'Reilly\\path ? :not_a_param"), 2),
                        (":b", Value::Bool(false), 5),
                        (":n", Value::Null, 0),
                    ] {
                        php.push_str(&format!(
                            "$s->bindValue('{}',{},{});\n",
                            name,
                            php_literal(&value),
                            kind
                        ));
                        harness
                            .call(
                                &statement,
                                "bindValue",
                                vec![Value::from(name), value, Value::Int(kind)],
                            )
                            .await
                            .unwrap();
                    }
                    php.push_str("$s->execute();echo enc($s->fetch(2)),\"\\n\";\n");
                    harness.call(&statement, "execute", vec![]).await.unwrap();
                    actual.push(encode(
                        &harness
                            .call(&statement, "fetch", vec![Value::Int(2)])
                            .await
                            .unwrap(),
                    ));
                }
                for mode in [12, 34, 66, 35, 67] {
                    let query =
                        "SELECT 1 AS k, 'a' AS v UNION ALL SELECT 1, 'b' UNION ALL SELECT 2, 'c'";
                    let statement = object(
                        harness
                            .call(&pdo, "query", vec![Value::from(query)])
                            .await
                            .unwrap(),
                    );
                    actual.push(encode(
                        &harness
                            .call(&statement, "fetchAll", vec![Value::Int(mode)])
                            .await
                            .unwrap(),
                    ));
                    php.push_str(&format!(
                        "echo enc($p->query({})->fetchAll({mode})),\"\\n\";\n",
                        php_literal(&Value::from(query))
                    ));
                }
                let alter_mode = if driver == "pgsql" {
                    "SET standard_conforming_strings=off"
                } else {
                    "SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'"
                };
                for altered in [false, true] {
                    if altered {
                        harness
                            .call(&pdo, "exec", vec![Value::from(alter_mode)])
                            .await
                            .unwrap();
                        php.push_str(&format!(
                            "$p->exec({});\n",
                            php_literal(&Value::from(alter_mode))
                        ));
                    }
                    let value = Value::from("O'Reilly\\path");
                    actual.push(encode(
                        &harness
                            .call(&pdo, "quote", vec![value.clone()])
                            .await
                            .unwrap(),
                    ));
                    php.push_str(&format!(
                        "echo enc($p->quote({})),\"\\n\";\n",
                        php_literal(&value)
                    ));
                }
                let expected = Command::new(support::PHP)
                    .args(["-r", &php])
                    .output()
                    .unwrap();
                assert!(
                    expected.status.success(),
                    "{}",
                    String::from_utf8_lossy(&expected.stderr)
                );
                let expected = String::from_utf8(expected.stdout).unwrap();
                let actual = format!("{}\n", actual.join("\n"));
                std::fs::write(
                    std::path::Path::new(support::SCRATCH)
                        .join(format!("native-parameters-{driver}.php.out")),
                    &expected,
                )
                .unwrap();
                assert_eq!(actual, expected, "{driver}");
            }
        })
        .await;
}
