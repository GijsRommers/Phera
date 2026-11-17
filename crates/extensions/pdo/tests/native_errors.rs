//! `$pdo->errorInfo()` retains the driver diagnostic in silent mode and resets on success.

use std::process::Command;

use phera_extension_api::Value;
use phera_extension_api::testing::oracle::{PHP_PREAMBLE, encode, php_literal};

mod native_support;
mod support;
use native_support::{Harness, object};

#[tokio::test(flavor = "current_thread")]
async fn silent_driver_errors_match_php() {
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
        for method in ["errorCode", "errorInfo"] {
            php.push_str(&format!("echo enc($p->{method}()),\"\\n\";\n"));
            actual.push(encode(&harness.call(&pdo, method, vec![]).await.unwrap()));
        }
        harness
            .call(&pdo, "setAttribute", vec![Value::Int(3), Value::Int(0)])
            .await
            .unwrap();
        php.push_str("$p->setAttribute(3,0);\n");
        for query in [
            "CREATE TEMPORARY TABLE phera_error_oracle (id INT PRIMARY KEY)",
            "INSERT INTO phera_error_oracle VALUES (1)",
            "INSERT INTO phera_error_oracle VALUES (1)",
            "DELETE FROM phera_missing_error_oracle",
            "UPDATE phera_error_oracle SET id=1",
            "SELECT 1",
            "UPDATE phera_error_oracle SET id=1",
        ] {
            php.push_str(&format!(
                "echo enc($p->exec({})),\"\\n\";\n",
                php_literal(&Value::from(query))
            ));
            actual.push(encode(
                &harness
                    .call(&pdo, "exec", vec![Value::from(query)])
                    .await
                    .unwrap(),
            ));
            for method in ["errorCode", "errorInfo"] {
                php.push_str(&format!("echo enc($p->{method}()),\"\\n\";\n"));
                actual.push(encode(&harness.call(&pdo, method, vec![]).await.unwrap()));
            }
        }
        // Exercise a statement as well as the connection's error state.
        let statement = object(
            harness
                .call(
                    &pdo,
                    "prepare",
                    vec![Value::from("SELECT id FROM phera_error_oracle")],
                )
                .await
                .unwrap(),
        );
        php.push_str("$s=$p->prepare('SELECT id FROM phera_error_oracle');\n");
        for method in [
            "errorCode",
            "errorInfo",
            "execute",
            "errorCode",
            "errorInfo",
        ] {
            php.push_str(&format!("echo enc($s->{method}()),\"\\n\";\n"));
            actual.push(encode(
                &harness.call(&statement, method, vec![]).await.unwrap(),
            ));
        }
        let expected = Command::new(support::php())
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
            support::scratch().join(format!("native-errors-{driver}.php.out")),
            &expected,
        )
        .unwrap();
        assert_eq!(actual, expected, "{driver}");
    }
}
