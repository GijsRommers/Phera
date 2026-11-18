//! Every PostgreSQL and MySQL script in `tests/scripts` runs through PHP 8.5 and through Phera,
//! against both test databases, and must print the same bytes.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, php_scripts};

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
