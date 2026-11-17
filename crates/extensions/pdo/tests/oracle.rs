//! Runs the same PHP script on PHP 8.5 and Phera and compares every output byte.

use std::path::Path;
use std::process::Command;

mod support;

#[test]
fn php_scripts_match_phera() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN to the clean-copy phera executable");
        return;
    };
    if !support::databases_available() {
        return;
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let mut scripts: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "php"))
        .collect();
    scripts.sort();
    let output_dir = Path::new(support::SCRATCH).join("oracle");
    std::fs::create_dir_all(&output_dir).unwrap();
    let mut failures = Vec::new();
    for script in &scripts {
        let numeric;
        let script = if std::env::var_os("PHERA_PDO_NUMERIC_CONSTANTS").is_some() {
            let directory = Path::new(support::SCRATCH).join("numeric-oracle");
            std::fs::create_dir_all(&directory).unwrap();
            numeric = directory.join(script.file_name().unwrap());
            let replace = r#"$c=(new ReflectionClass(PDO::class))->getConstants();
                echo preg_replace_callback('/PDO::([A-Z_]+)/',
                    fn($m)=>var_export($c[$m[1]],true),file_get_contents($argv[1]));"#;
            let converted = Command::new(support::PHP)
                .args(["-r", replace])
                .arg(script)
                .output()
                .unwrap();
            assert!(converted.status.success());
            std::fs::write(&numeric, converted.stdout).unwrap();
            &numeric
        } else {
            script
        };
        let php = Command::new(support::PHP)
            .args(["-d", "display_errors=1", "-d", "log_errors=0"])
            .arg(script)
            .output()
            .unwrap();
        let phera = Command::new(&binary).arg(script).output().unwrap();
        let name = script.file_stem().unwrap().to_string_lossy();
        std::fs::write(output_dir.join(format!("{name}.php.out")), &php.stdout).unwrap();
        std::fs::write(output_dir.join(format!("{name}.phera.out")), &phera.stdout).unwrap();
        std::fs::write(output_dir.join(format!("{name}.phera.err")), &phera.stderr).unwrap();
        let matches = php.status.success() && phera.status.success() && php.stdout == phera.stdout;
        eprintln!("{name}: {}", if matches { "MATCH" } else { "FAIL" });
        if !matches {
            failures.push(name.into_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scripts failed: {}. Outputs: {}",
        failures.len(),
        scripts.len(),
        failures.join(", "),
        output_dir.display()
    );
}
