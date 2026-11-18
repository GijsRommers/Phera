//! Where the test databases are: `new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test')`.
//! Start them as described in ../README.md; never port 5432.

use phera_extension_api::testing::oracle::run_php;

pub fn dsn(driver: &str) -> String {
    let port = match driver {
        "pgsql" => 54329,
        "mysql" => 33069,
        _ => panic!("unknown driver {driver}"),
    };
    format!("{driver}:host=127.0.0.1;port={port};dbname=phera_test")
}

/// Whether PHP and both databases are there; a test without them skips.
pub fn databases_available() -> bool {
    let probe = ["pgsql", "mysql"]
        .map(|driver| {
            format!(
                "try {{ new PDO('{}','phera','phera'); echo 'ok'; }} catch (Throwable $e) {{}}",
                dsn(driver)
            )
        })
        .join("\n");
    let available = run_php("pdo-databases", &format!("<?php\n{probe}"))
        .is_some_and(|lines| lines.concat() == "okok");
    if !available {
        eprintln!("SKIP: PHP 8.5 or the PDO test databases are unavailable");
    }
    available
}
