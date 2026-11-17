//! PHP oracle for registered native methods, independently of the CLI registration blocker.

use std::process::Command;

use phera_extension_api::Value;
use phera_extension_api::testing::oracle::{PHP_PREAMBLE, encode, php_literal};

mod native_support;
mod support;
use native_support::{Harness, object};

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

#[tokio::test(flavor = "current_thread")]
async fn native_fetch_types_and_modes_match_php() {
    if !support::databases_available() {
        return;
    }
    for driver in ["pgsql", "mysql"] {
        let mut harness = Harness::new();
        let pdo = harness.connect(driver).await;
        let mut php = PHP_PREAMBLE.to_owned();
        php.push_str(&format!(
            "$p=new PDO('{}','phera','phera');\n",
            support::dsn(driver)
        ));
        let mut actual = Vec::new();
        let mut labels = Vec::new();
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
            for stringify in [false, true] {
                harness
                    .call(
                        &pdo,
                        "setAttribute",
                        vec![Value::Int(17), Value::Bool(stringify)],
                    )
                    .await
                    .unwrap();
                php.push_str(&format!("$p->setAttribute(17,{});\n", i32::from(stringify)));
                for query in queries(driver) {
                    for mode in [2, 3, 4, 5, 7] {
                        let label = format!(
                            "{driver} emulate={emulate} stringify={stringify} mode={mode}: {query}"
                        );
                        let sql = php_literal(&Value::from(query));
                        php.push_str(&format!(
                            "echo enc($p->query({sql})->fetchAll({mode})),\"\\n\";\n"
                        ));
                        let statement = object(
                            harness
                                .call(&pdo, "query", vec![Value::from(query)])
                                .await
                                .unwrap_or_else(|error| panic!("{label}: {error:?}")),
                        );
                        let result = harness
                            .call(&statement, "fetchAll", vec![Value::Int(mode)])
                            .await
                            .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                        actual.push(encode(&result));
                        labels.push(label);
                    }
                }
            }
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
        let expected: Vec<_> = expected.lines().collect();
        let differences: Vec<_> = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(|(_, (actual, expected))| actual != expected)
            .map(|(index, (actual, expected))| {
                format!("{}\nPHP {expected}\nPDO {actual}", labels[index])
            })
            .collect();
        let log = std::path::Path::new(support::SCRATCH).join(format!("native-{driver}.log"));
        std::fs::write(log, differences.join("\n")).unwrap();
        assert_eq!(actual.len(), expected.len());
        assert!(
            differences.is_empty(),
            "{} mismatches of {}: {}",
            differences.len(),
            actual.len(),
            differences.join("\n")
        );
        eprintln!(
            "{driver}: {} native-method oracle comparisons matched",
            actual.len()
        );
    }
}
