//! Every PostgreSQL and MySQL script in `tests/scripts` runs through PHP 8.5 and through Phera,
//! against both test databases, and must print the same bytes.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, Runtime, php_scripts};

mod support;

#[test]
fn php_scripts_match_phera() {
    let Some(comparison) = Comparison::from_environment("pdo") else {
        return;
    };
    if !support::databases_available() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    // The sqlite_ scripts need a directory of their own; see sqlite_oracle.rs.
    let mut scripts = php_scripts(&directory, "mysql_");
    scripts.extend(php_scripts(&directory, "pgsql_"));
    comparison.compare_all(&scripts, |_, _, _| {});
}

#[test]
fn postgres_emulation_uses_postgres_comment_and_literal_boundaries() {
    let Some(comparison) = Comparison::from_environment("pdo-pgsql-scanner") else {
        return;
    };
    if !support::databases_available() {
        return;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scripts/postgres_scanner_differences.php");
    let output = comparison
        .command(Runtime::Phera, &script)
        .output()
        .expect("run the PostgreSQL scanner regressions");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "nested comment 1\ncomment binding HY093\nunterminated literal HY093\n"
    );
    assert!(output.stderr.is_empty());
}
