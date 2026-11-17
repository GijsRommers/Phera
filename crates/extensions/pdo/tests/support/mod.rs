//! Test database locations: `new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test')`.

use std::process::Command;

pub const PHP: &str = "/opt/homebrew/bin/php";
pub const SCRATCH: &str = concat!(
    "/private/tmp/claude-501/-Users-gijsr--t3-worktrees-phera-t3-f1d03fb9/",
    "8823bb04-57b9-49e9-a9e2-fe9f0dede31c/scratchpad/pdo"
);

pub fn dsn(driver: &str) -> String {
    let port = match driver {
        "pgsql" => 54329,
        "mysql" => 33069,
        _ => panic!("Unknown driver"),
    };
    format!("{driver}:host=127.0.0.1;port={port};dbname=phera_test")
}

pub fn databases_available() -> bool {
    let mut available = true;
    for driver in ["pgsql", "mysql"] {
        let script = format!(
            "try {{new PDO('{}','phera','phera');}} catch (Throwable $e) {{exit(1);}}",
            dsn(driver)
        );
        let result = Command::new(PHP).args(["-r", &script]).output();
        if !result.is_ok_and(|output| output.status.success()) {
            available = false;
        }
    }
    if !available {
        eprintln!("SKIP: PDO test databases or PHP 8.5 are unavailable");
    }
    available
}
