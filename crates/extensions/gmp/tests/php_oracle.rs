//! Run GMP functions and operators with the PHP CLI and compare their outcomes.

use std::io::BufRead;

use phera_extension_api::testing::{oracle::PHP_PREAMBLE, scripts::Comparison};

#[test]
fn gmp_matches_php() {
    let Some(comparison) = Comparison::from_environment("gmp") else {
        return;
    };
    let script = comparison.artifacts.join("gmp.php");
    let preamble = PHP_PREAMBLE.replace(
        "    if (is_object($v)) {",
        "    if (is_object($v) && get_class($v) === 'GMP') return 'G:' . (string) $v;\n    if (is_object($v)) {",
    );
    std::fs::write(
        &script,
        format!("<?php\n{preamble}{}", include_str!("matrix.php")),
    )
    .expect("write GMP oracle program");
    let matched = comparison
        .compare(&script, |_, _| {})
        .expect("PHP and Phera agree");
    let calls = matched.stdout.lines().count();
    println!("{calls} GMP calls matched");
    assert!(calls > 8_000);
}
