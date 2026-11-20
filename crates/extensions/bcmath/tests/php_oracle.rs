//! Compare decimal calls, exceptions, and diagnostics with PHP's BCMath module.

use phera_extension_api::testing::{oracle::PHP_PREAMBLE, scripts::Comparison};

#[test]
fn bcmath_matches_php() {
    let Some(comparison) = Comparison::from_environment("bcmath") else {
        return;
    };
    let script = comparison.artifacts.join("bcmath.php");
    let mut source = String::from("<?php\n");
    source.push_str(PHP_PREAMBLE);
    source.push_str(include_str!("matrix.php"));
    std::fs::write(&script, source).expect("write BCMath oracle program");
    let matched = comparison
        .compare(&script, |_, _| {})
        .expect("PHP and Phera agree");
    let calls = matched.stdout.lines().count();
    println!("{calls} BCMath calls matched");
    assert!(calls > 9_000);
}

use std::io::BufRead;
