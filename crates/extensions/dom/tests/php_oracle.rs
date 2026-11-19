//! Run the same PHP operations against real PHP 8.5 and the Phera CLI.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, Runtime};

#[test]
fn document_matrix() {
    compare("matrix");
}

#[test]
fn termwind_document_calls() {
    compare("termwind");
}

#[test]
fn malformed_xml_recovery() {
    compare("recovery");
}

#[test]
fn legacy_collection_and_mutation_edges() {
    compare("legacy_edges");
}

fn compare(name: &str) {
    let Some(oracle) = Comparison::from_environment("dom") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scripts")
        .join(format!("{name}.php"));
    let php = oracle
        .command(Runtime::Php, &script)
        .output()
        .expect("run PHP");
    let phera = oracle
        .command(Runtime::Phera, &script)
        .output()
        .expect("run Phera");
    let output = oracle.check(name, &php, &phera);
    let actual = String::from_utf8_lossy(&phera.stdout);
    let expected = String::from_utf8_lossy(&php.stdout);
    let actual: Vec<_> = actual.lines().collect();
    let expected: Vec<_> = expected.lines().collect();
    let mut failures = 0;
    for (index, line) in expected.iter().enumerate() {
        if actual.get(index) != Some(line) {
            failures += 1;
            if failures <= 12 {
                eprintln!(
                    "{}\nPHP:   {}\nPhera: {}",
                    index + 1,
                    line,
                    actual.get(index).copied().unwrap_or("<missing line>")
                );
            }
        }
    }
    failures += actual.len().saturating_sub(expected.len());
    eprintln!(
        "{name}: {} compared rows, {failures} mismatches",
        expected.len()
    );
    if let Err(error) = output {
        panic!("{error}; outputs saved in {}", oracle.artifacts.display());
    }
}
