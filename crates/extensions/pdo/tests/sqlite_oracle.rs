//! Every SQLite script runs through the release CLI and the real PHP 8.5 oracle.

use std::path::Path;
use std::process::Command;

#[test]
fn sqlite_scripts_match_php() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for SQLite CLI comparisons");
        return;
    };
    let php = phera_extension_api::testing::oracle::find_php().expect("PHP oracle required");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let mut scripts: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sqlite_")
        })
        .collect();
    scripts.sort();
    let root = std::env::temp_dir().join(format!("phera-sqlite-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut failures = Vec::new();
    for script in &scripts {
        let name = script.file_stem().unwrap().to_string_lossy();
        let mut outputs = Vec::new();
        for (label, executable) in [("php", php.as_os_str()), ("phera", binary.as_ref())] {
            let data = root.join(format!("{name}-{label}"));
            std::fs::create_dir_all(&data).unwrap();
            let mut command = Command::new(executable);
            if label == "php" {
                command.args(["-n", "-d", "display_errors=1", "-d", "log_errors=0"]);
            }
            let output = command
                .current_dir(&data)
                .arg(script)
                .arg(&data)
                .output()
                .unwrap();
            std::fs::write(root.join(format!("{name}.{label}.out")), &output.stdout).unwrap();
            std::fs::write(root.join(format!("{name}.{label}.err")), &output.stderr).unwrap();
            outputs.push(output);
        }
        let matches =
            outputs.iter().all(|o| o.status.success()) && outputs[0].stdout == outputs[1].stdout;
        eprintln!("{name}: {}", if matches { "MATCH" } else { "FAIL" });
        if !matches {
            failures.push(name.into_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {} mismatches: {}. Artifacts: {}",
        failures.len(),
        scripts.len(),
        failures.join(", "),
        root.display()
    );
    eprintln!(
        "{} SQLite scripts, zero mismatches; {}",
        scripts.len(),
        root.display()
    );
}
