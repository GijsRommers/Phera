//! Compare uploaded content and the fileinfo API with real PHP.

use std::io::BufRead;
use std::path::Path;

use phera_extension_api::testing::{oracle::PHP_PREAMBLE, scripts::Comparison};

#[test]
fn fileinfo_matches_php() {
    let Some(comparison) = Comparison::from_environment("fileinfo") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let corpus = root.join("corpus");
    let matrix = comparison
        .compare(&root.join("matrix.php"), |_, command| {
            command.arg(&corpus);
        })
        .expect("compare descriptions, MIME, encodings and extensions for paths and buffers");
    assert_eq!(
        matrix.stdout,
        include_bytes!("php-8.5.jsonl"),
        "the live PHP answer changed from the recorded corpus"
    );
    let api = comparison.artifacts.join("api.php");
    std::fs::write(
        &api,
        format!("<?php\n{PHP_PREAMBLE}\n{}", include_str!("api.php")),
    )
    .expect("write API matrix");
    let calls = comparison
        .compare(&api, |_, command| {
            command.arg(&corpus);
        })
        .expect("compare finfo state, stream positions, return values and diagnostics");
    let count = calls.stdout.lines().count();
    let corpus_calls = matrix.stdout.lines().count() * 2;
    println!("{corpus_calls} corpus calls and {count} API cases matched");
    assert!(corpus_calls >= 550 && count >= 60);
}
