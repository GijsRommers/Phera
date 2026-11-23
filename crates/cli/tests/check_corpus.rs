//! Every checker corpus fixture is exercised through the public CLI at its expected levels.
//! Expected line sets live beside each PHP file and are independent of output wording.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[test]
fn omitted_items_have_one_level_zero_diagnostic_each() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../analyzer/tests/corpus/not_supported_calls.php");
    let output = Command::new(env!("CARGO_BIN_EXE_phera"))
        .args(["check", "--no-progress", "--level", "0"])
        .arg(fixture)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    let diagnostics: Vec<_> = text.lines().filter(|line| line.contains("  [")).collect();
    assert_eq!(diagnostics.len(), 7, "{text}");
    assert!(
        diagnostics
            .iter()
            .all(|line| line.contains("[phera.notSupported]")),
        "{text}"
    );
    assert!(text.contains(
        "date_sunset() is not supported by Phera: deprecated since PHP 8.1, use date_sun_info() instead"
    ));
}

#[test]
fn corpus_lines_at_each_level() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../analyzer/tests/corpus");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "php"))
        .collect();
    files.sort();
    assert!(files.len() >= 300);
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    let failures: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(files.len().div_ceil(workers))
            .map(|chunk| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for file in chunk {
                        let source = std::fs::read_to_string(file).unwrap();
                        assert!(source.lines().nth(1).unwrap().starts_with("// Rule:"));
                        let expected =
                            std::fs::read_to_string(file.with_extension("expect")).unwrap();
                        for (level, line) in expected.lines().enumerate() {
                            let expected = numbers(line.split_once(':').unwrap().1.trim());
                            let output = Command::new(env!("CARGO_BIN_EXE_phera"))
                                .args(["check", "--no-progress", "--level", &level.to_string()])
                                .arg(file)
                                .output()
                                .unwrap();
                            let text = String::from_utf8(output.stdout).unwrap();
                            let actual: BTreeSet<usize> = text
                                .lines()
                                .filter_map(|line| {
                                    line.trim_start()
                                        .split_once("  [")
                                        .and_then(|(n, _)| n.parse().ok())
                                })
                                .collect();
                            if actual != expected
                                || output.status.code() != Some(i32::from(!actual.is_empty()))
                            {
                                failures.push(format!(
                                    "{} level {level}: expected {expected:?}, got \
                                {actual:?}\n{text}{}",
                                    file.display(),
                                    String::from_utf8_lossy(&output.stderr)
                                ));
                            }
                        }
                    }
                    failures
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn numbers(text: &str) -> BTreeSet<usize> {
    text.split(',')
        .filter(|n| !n.is_empty())
        .map(|n| n.parse().unwrap())
        .collect()
}
