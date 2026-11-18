//! The registered PDO methods, called directly, against PHP 8.5 running the
//! same calls: error state, bound values and quoting, and the value types
//! and fetch modes of both drivers.

use phera_extension_api::testing::oracle::{PHP_PREAMBLE, compare, encode, php_literal, run_php};
use phera_extension_api::{NativeResult, Value};

mod native_support;
mod support;
use native_support::{Harness, object};

/// A PHP script that makes the same calls as a test, and the encoded results
/// of both sides, line for line.
struct Oracle {
    script: String,
    labels: Vec<String>,
    phera: Vec<String>,
}

impl Oracle {
    /// The script starts with `$p = new PDO(...)` for `driver`.
    fn new(driver: &str) -> Self {
        Self {
            script: format!(
                "<?php\n{PHP_PREAMBLE}\n$p=new PDO('{}','phera','phera');\n",
                support::dsn(driver)
            ),
            labels: Vec::new(),
            phera: Vec::new(),
        }
    }

    /// PHP that changes state the next comparisons depend on.
    fn run(&mut self, php: &str) {
        self.script.push_str(php);
        self.script.push('\n');
    }

    /// PHP's value of `expression` against what Phera's call returned.
    fn compare(&mut self, expression: &str, phera: NativeResult) {
        let phera = phera.unwrap_or_else(|error| panic!("{expression}: {error:?}"));
        self.script
            .push_str(&format!("echo enc({expression}), \"\\n\";\n"));
        self.labels.push(expression.to_owned());
        self.phera.push(encode(&phera));
    }

    fn check(self, name: &str) {
        let php = run_php(name, &self.script).expect("PHP 8.5 oracle");
        compare(name, &self.labels, &php, &self.phera);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn silent_driver_errors_match_php() {
    if !support::databases_available() {
        return;
    }
    for driver in ["pgsql", "mysql"] {
        let mut harness = Harness::new();
        let mut oracle = Oracle::new(driver);
        let pdo = harness.connect(driver).await;
        for method in ["errorCode", "errorInfo"] {
            let result = harness.call(&pdo, method, vec![]).await;
            oracle.compare(&format!("$p->{method}()"), result);
        }
        harness
            .call(&pdo, "setAttribute", vec![Value::Int(3), Value::Int(0)])
            .await
            .unwrap();
        oracle.run("$p->setAttribute(3,0);");
        for query in [
            "CREATE TEMPORARY TABLE phera_error_oracle (id INT PRIMARY KEY)",
            "INSERT INTO phera_error_oracle VALUES (1)",
            "INSERT INTO phera_error_oracle VALUES (1)",
            "DELETE FROM phera_missing_error_oracle",
            "UPDATE phera_error_oracle SET id=1",
            "SELECT 1",
            "UPDATE phera_error_oracle SET id=1",
        ] {
            let result = harness.call(&pdo, "exec", vec![Value::from(query)]).await;
            oracle.compare(
                &format!("$p->exec({})", php_literal(&Value::from(query))),
                result,
            );
            for method in ["errorCode", "errorInfo"] {
                let result = harness.call(&pdo, method, vec![]).await;
                oracle.compare(&format!("$p->{method}()"), result);
            }
        }
        // A statement keeps an error state of its own.
        let query = "SELECT id FROM phera_error_oracle";
        let statement = object(
            harness
                .call(&pdo, "prepare", vec![Value::from(query)])
                .await
                .unwrap(),
        );
        oracle.run(&format!("$s=$p->prepare('{query}');"));
        for method in [
            "errorCode",
            "errorInfo",
            "execute",
            "errorCode",
            "errorInfo",
        ] {
            let result = harness.call(&statement, method, vec![]).await;
            oracle.compare(&format!("$s->{method}()"), result);
        }
        oracle.check(&format!("pdo-errors-{driver}"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn bindings_quoting_and_grouping_match_php() {
    if !support::databases_available() {
        return;
    }
    for driver in ["pgsql", "mysql"] {
        let mut harness = Harness::new();
        let mut oracle = Oracle::new(driver);
        let pdo = harness.connect(driver).await;
        for emulate in [false, true] {
            harness
                .call(
                    &pdo,
                    "setAttribute",
                    vec![Value::Int(20), Value::Bool(emulate)],
                )
                .await
                .unwrap();
            oracle.run(&format!("$p->setAttribute(20,{});", i32::from(emulate)));
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
            oracle.run(&format!(
                "$s=$p->prepare({});",
                php_literal(&Value::from(query))
            ));
            for (name, value, kind) in [
                (":i", Value::from("42"), 1),
                (":s", Value::from("é😀 O'Reilly\\path ? :not_a_param"), 2),
                (":b", Value::Bool(false), 5),
                (":n", Value::Null, 0),
            ] {
                oracle.run(&format!(
                    "$s->bindValue('{name}',{},{kind});",
                    php_literal(&value)
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
            oracle.run("$s->execute();");
            harness.call(&statement, "execute", vec![]).await.unwrap();
            let result = harness.call(&statement, "fetch", vec![Value::Int(2)]).await;
            oracle.compare("$s->fetch(2)", result);
        }
        // PDO::FETCH_KEY_PAIR, and FETCH_GROUP and FETCH_UNIQUE with
        // FETCH_COLUMN and FETCH_NUM.
        for mode in [12, 34, 66, 35, 67] {
            let query = "SELECT 1 AS k, 'a' AS v UNION ALL SELECT 1, 'b' UNION ALL SELECT 2, 'c'";
            let statement = object(
                harness
                    .call(&pdo, "query", vec![Value::from(query)])
                    .await
                    .unwrap(),
            );
            let result = harness
                .call(&statement, "fetchAll", vec![Value::Int(mode)])
                .await;
            oracle.compare(
                &format!(
                    "$p->query({})->fetchAll({mode})",
                    php_literal(&Value::from(query))
                ),
                result,
            );
        }
        // Quoting follows the session's escaping mode.
        let escaping = if driver == "pgsql" {
            "SET standard_conforming_strings=off"
        } else {
            "SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'"
        };
        let value = Value::from("O'Reilly\\path");
        for changed in [false, true] {
            if changed {
                harness
                    .call(&pdo, "exec", vec![Value::from(escaping)])
                    .await
                    .unwrap();
                oracle.run(&format!(
                    "$p->exec({});",
                    php_literal(&Value::from(escaping))
                ));
            }
            let result = harness.call(&pdo, "quote", vec![value.clone()]).await;
            oracle.compare(&format!("$p->quote({})", php_literal(&value)), result);
        }
        oracle.check(&format!("pdo-parameters-{driver}"));
    }
}

fn queries(driver: &str) -> Vec<&'static str> {
    let mut queries = vec![
        "SELECT 1 AS id, 'hello' AS label, NULL AS missing",
        "SELECT 1 AS id UNION ALL SELECT 2 AS id",
        "SELECT 1 AS id WHERE 1=0",
        "SELECT 1 AS x, 2 AS x",
        "SELECT 1.250 AS decimal_value",
    ];
    if driver == "pgsql" {
        queries.extend([
            "SELECT true AS yes, false AS no",
            "SELECT 32767::int2, 2147483647::int4, 9223372036854775807::int8",
            "SELECT 1.5::float4 AS f, 2.25::float8 AS d",
            "SELECT 0.000123::numeric, -12345678.90001::numeric, 10000.00::numeric",
            "SELECT '2024-01-02 03:04:05.123'::timestamp AS t, '2024-01-02'::date AS d",
            "SELECT '{\"x\":1}'::json AS j, '{\"x\":1}'::jsonb AS jb",
            "SELECT ARRAY[1,2,NULL] AS a, ARRAY['a,b', 'NULL', ''] AS s",
            "SELECT ARRAY[[1,2],[3,4]] AS a, '[0:1]={1,2}'::int[] AS b",
        ]);
    } else {
        queries.extend([
            "SELECT true AS yes, false AS no",
            "SELECT CAST(9223372036854775807 AS SIGNED) AS i",
            "SELECT CAST(18446744073709551615 AS UNSIGNED) AS u",
            "SELECT 1.5e0 AS f, 2.25e0 AS d",
            "SELECT CAST('2024-01-02' AS DATE) AS d",
            "SELECT CAST('2024-01-02 03:04:05' AS DATETIME) AS t",
            "SELECT CAST('{\"x\":1}' AS JSON) AS j",
            "SELECT -12345678.90001 AS d, 0.000123 AS s",
        ]);
    }
    queries
}

/// Every query crossed with emulated and native prepares, with and without
/// `ATTR_STRINGIFY_FETCHES`, and with FETCH_ASSOC, NUM, BOTH, OBJ and COLUMN.
#[tokio::test(flavor = "current_thread")]
async fn native_fetch_types_and_modes_match_php() {
    if !support::databases_available() {
        return;
    }
    for driver in ["pgsql", "mysql"] {
        let mut harness = Harness::new();
        let mut oracle = Oracle::new(driver);
        let pdo = harness.connect(driver).await;
        for emulate in [false, true] {
            harness
                .call(
                    &pdo,
                    "setAttribute",
                    vec![Value::Int(20), Value::Bool(emulate)],
                )
                .await
                .unwrap();
            oracle.run(&format!("$p->setAttribute(20,{});", i32::from(emulate)));
            for stringify in [false, true] {
                harness
                    .call(
                        &pdo,
                        "setAttribute",
                        vec![Value::Int(17), Value::Bool(stringify)],
                    )
                    .await
                    .unwrap();
                oracle.run(&format!("$p->setAttribute(17,{});", i32::from(stringify)));
                for query in queries(driver) {
                    for mode in [2, 3, 4, 5, 7] {
                        let statement = object(
                            harness
                                .call(&pdo, "query", vec![Value::from(query)])
                                .await
                                .unwrap(),
                        );
                        let result = harness
                            .call(&statement, "fetchAll", vec![Value::Int(mode)])
                            .await;
                        oracle.compare(
                            &format!(
                                "$p->query({})->fetchAll({mode})",
                                php_literal(&Value::from(query))
                            ),
                            result,
                        );
                    }
                }
            }
        }
        oracle.check(&format!("pdo-fetch-{driver}"));
    }
}
