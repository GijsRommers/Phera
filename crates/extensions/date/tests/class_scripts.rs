//! Compare complete scripts through the release CLI and PHP's own date extension.

use std::path::Path;
use std::process::Command;

use phera_extension_api::testing::oracle::find_php;

#[test]
fn class_scripts_match_php() {
    let Some(binary) = std::env::var_os("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN to the release Phera executable");
        return;
    };
    let php = find_php().expect("PHP 8.5 oracle required");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let mut scripts: Vec<_> = std::fs::read_dir(directory)
        .expect("script directory")
        .map(|entry| entry.expect("script").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "php"))
        .collect();
    scripts.sort();
    let evidence = std::env::temp_dir().join("phera-date-class-oracle");
    std::fs::create_dir_all(&evidence).expect("oracle evidence directory");
    let mut failures = Vec::new();
    for script in &scripts {
        let expected = Command::new(&php)
            .arg("-n")
            .arg(script)
            .output()
            .expect("PHP");
        let actual = Command::new(&binary).arg(script).output().expect("Phera");
        if !expected.status.success()
            || !actual.status.success()
            || expected.stdout != actual.stdout
            || expected.stderr != actual.stderr
        {
            let name = script.file_stem().expect("script name").to_string_lossy();
            for (runtime, output) in [("php", expected), ("phera", actual)] {
                std::fs::write(
                    evidence.join(format!("{name}.{runtime}.out")),
                    output.stdout,
                )
                .expect("stdout");
                std::fs::write(
                    evidence.join(format!("{name}.{runtime}.err")),
                    output.stderr,
                )
                .expect("stderr");
            }
            failures.push(name.into_owned());
        }
    }
    eprintln!(
        "Compared {} scripts, {} mismatches",
        scripts.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{failures:?}. Evidence: {}",
        evidence.display()
    );
}
