//! Test database locations: `new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test')`.

use std::process::Command;

pub fn php() -> std::path::PathBuf {
    phera_extension_api::testing::oracle::find_php().expect("PHP 8.5 oracle required")
}

pub fn scratch() -> std::path::PathBuf {
    let directory = std::env::temp_dir().join("phera-pdo-tests");
    std::fs::create_dir_all(&directory).expect("create PDO test artifacts directory");
    directory
}

pub fn dsn(driver: &str) -> String {
    let port = match driver {
        "pgsql" => 54329,
        "mysql" => 33069,
        _ => panic!("Unknown driver"),
    };
    format!("{driver}:host=127.0.0.1;port={port};dbname=phera_test")
}

pub fn databases_available() -> bool {
    if phera_extension_api::testing::oracle::find_php().is_none() {
        eprintln!("SKIP: PDO test databases or PHP 8.5 are unavailable");
        return false;
    }
    let mut available = true;
    for driver in ["pgsql", "mysql"] {
        let script = format!(
            "try {{new PDO('{}','phera','phera');}} catch (Throwable $e) {{exit(1);}}",
            dsn(driver)
        );
        let result = Command::new(php()).args(["-r", &script]).output();
        if !result.is_ok_and(|output| output.status.success()) {
            available = false;
        }
    }
    if !available {
        eprintln!("SKIP: PDO test databases or PHP 8.5 are unavailable");
    }
    available
}
