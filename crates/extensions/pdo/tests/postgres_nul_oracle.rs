//! PHP 8.5 observes C-string truncation. Phera may reject the input, but an
//! error must leave the same connection and prepared statement usable.

use phera_extension_api::testing::scripts::{Comparison, Runtime};

mod support;

const SCRIPT: &str = r#"<?php
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
$pid = $pdo->query('SELECT pg_backend_pid()')->fetchColumn();
function probe(PDO $pdo, int $pid, string $label, callable $run): void {
    try {
        $result = $run();
    } catch (PDOException $error) {
        $result = $error->getCode();
    }
    $same = $pid === $pdo->query('SELECT pg_backend_pid()')->fetchColumn();
    echo json_encode([$label, $result, $same]), "\n";
}
probe($pdo, $pid, 'SQL', fn() => $pdo->query("SELECT 7\x00; SELECT 8")->fetchAll(PDO::FETCH_COLUMN));
foreach ([true, false] as $emulate) {
    $pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
    $statement = $pdo->prepare('SELECT CAST(? AS text)');
    probe($pdo, $pid, $emulate ? 'emulated' : 'native', function () use ($statement) {
        $statement->execute(["a\x00z"]);
        return bin2hex($statement->fetchColumn());
    });
    $statement->execute(['ok']);
    echo $statement->fetchColumn(), "\n";
}
"#;

#[test]
fn postgres_nul_input_does_not_execute_shortened_sql_or_retire_connection() {
    let Some(comparison) = Comparison::from_environment("pdo-pgsql-nul") else {
        return;
    };
    if !support::databases_available() {
        return;
    }
    let script = comparison.artifacts.join("nul.php");
    std::fs::write(&script, SCRIPT).expect("write NUL oracle");
    for (runtime, expected) in [
        (
            Runtime::Php,
            "[\"SQL\",[7],true]\n[\"emulated\",\"61\",true]\nok\n[\"native\",\"61\",true]\nok\n",
        ),
        (
            Runtime::Phera,
            "[\"SQL\",\"08006\",true]\n[\"emulated\",\"08006\",true]\nok\n[\"native\",\"22021\",true]\nok\n",
        ),
    ] {
        let output = comparison
            .command(runtime, &script)
            .output()
            .expect("run the NUL oracle");
        assert!(output.status.success(), "{} failed", runtime.label());
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert!(output.stderr.is_empty());
    }
}
