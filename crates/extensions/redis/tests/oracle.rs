//! The identical script runs in real PHP 8.5 and the release Phera executable.
use phera_extension_api::testing::oracle::find_php;
use std::path::Path;
use std::process::Command;

#[test]
fn php_scripts_match_phera() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("Release oracle requires PHERA_BIN; CI sets it in the end-to-end job");
        return;
    };
    let php = find_php().expect("PHP 8.5 oracle required");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = root.join("../../../target/redis-oracle");
    std::fs::create_dir_all(&output).unwrap();
    let mut scripts: Vec<_> = std::fs::read_dir(root.join("tests/scripts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "php"))
        .collect();
    scripts.sort();
    assert!(
        scripts.len() >= 150,
        "Redis requires at least 150 comparison scripts"
    );
    let mut failures = Vec::new();
    let mut calls = 0;
    for script in &scripts {
        let name = script.file_stem().unwrap().to_string_lossy();
        let prefix = format!("phera:{}:{name}:", std::process::id());
        let php = Command::new(&php)
            .args(["-n", "-d", "display_errors=1", "-d", "log_errors=0"])
            .env("PHERA_REDIS_PREFIX", &prefix)
            .arg(script)
            .arg(&prefix)
            .output()
            .unwrap();
        let phera = Command::new(&binary)
            .env("PHERA_REDIS_PREFIX", &prefix)
            .arg(script)
            .arg(&prefix)
            .output()
            .unwrap();
        for (suffix, bytes) in [
            ("php.out", &php.stdout),
            ("phera.out", &phera.stdout),
            ("php.err", &php.stderr),
            ("phera.err", &phera.stderr),
        ] {
            std::fs::write(output.join(format!("{name}.{suffix}")), bytes).unwrap();
        }
        calls += String::from_utf8_lossy(&php.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("CALLS=")?.parse::<usize>().ok())
            .sum::<usize>();
        if !php.status.success()
            || !phera.status.success()
            || php.stdout != phera.stdout
            || php.stderr != phera.stderr
        {
            failures.push(name.to_string());
        }
    }
    eprintln!(
        "{} scripts, {calls} compared calls, {} script mismatches; outputs {}",
        scripts.len(),
        failures.len(),
        output.display()
    );
    assert!(
        failures.is_empty(),
        "Mismatched scripts: {}",
        failures.join(", ")
    );
}
