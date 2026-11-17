//! Actual HTTP servers compare 200 concurrent requests, then isolate one worker's responsiveness.

use std::process::Command;

#[test]
fn sqlite_wal_concurrency_and_worker_responsiveness() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for SQLite HTTP concurrency");
        return;
    };
    let php = phera_extension_api::testing::oracle::find_php().expect("PHP required");
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tools/sqlite_concurrency.py");
    let output = Command::new("python3")
        .arg(script)
        .arg(binary)
        .arg(php)
        .output()
        .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
