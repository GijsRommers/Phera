//! Both runtimes must decode the other runtime's wire data.

use std::io::BufRead;
use std::path::PathBuf;

use phera_extension_api::testing::{
    oracle::PHP_PREAMBLE,
    scripts::{Comparison, Runtime},
};

fn script(comparison: &Comparison, name: &str, source: &str) -> PathBuf {
    let path = comparison.artifacts.join(format!("{name}.php"));
    std::fs::write(&path, format!("<?php\n{PHP_PREAMBLE}\n{source}")).expect("write oracle script");
    path
}

#[test]
fn zlib_matches_php() {
    let Some(comparison) = Comparison::from_environment("zlib") else {
        return;
    };
    let generate = script(&comparison, "produce", include_str!("produce.php"));
    let decode = script(&comparison, "decode", include_str!("decode.php"));
    let matrix = script(&comparison, "matrix", include_str!("matrix.php"));
    let php_wire = comparison.artifacts.join("php-wire");
    let phera_wire = comparison.artifacts.join("phera-wire");
    for (runtime, destination) in [(Runtime::Php, &php_wire), (Runtime::Phera, &phera_wire)] {
        let result = comparison
            .command(runtime, &generate)
            .arg(destination)
            .output()
            .expect("produce compressed bytes");
        assert!(
            result.status.success() && result.stdout.is_empty() && result.stderr.is_empty(),
            "{}: {:?}, {}, {}",
            runtime.label(),
            result.status,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr),
        );
    }
    let crossed = comparison
        .compare(&decode, |runtime, command| {
            command.arg(if runtime == Runtime::Php {
                &phera_wire
            } else {
                &php_wire
            });
        })
        .expect("each runtime decodes the other's compressed bytes");
    assert!(!crossed.stdout.windows(3).any(|bytes| bytes == b"=E:"));
    let cases = comparison
        .compare(&matrix, |runtime, command| {
            command
                .arg(&php_wire)
                .current_dir(comparison.scratch_directory(&matrix, runtime));
        })
        .expect("PHP and Phera agree on diagnostics, state, and file operations");
    let count = crossed.stdout.lines().count() + cases.stdout.lines().count();
    println!("{count} zlib cases matched in both runtimes");
    assert!(count > 3_000);
}

#[test]
fn inflate_limit_stops_two_gibibytes_of_output() {
    let Some(comparison) = Comparison::from_environment("zlib-limit") else {
        return;
    };
    let producer = script(
        &comparison,
        "produce-limit",
        include_str!("produce_limit.php"),
    );
    let consumer = script(
        &comparison,
        "decode-limit",
        include_str!("decode_limit.php"),
    );
    let wire = comparison.artifacts.join("limit.gz");
    let generated = comparison
        .command(Runtime::Php, &producer)
        .arg(&wire)
        .output()
        .expect("generate gzip stream with PHP");
    assert!(generated.status.success() && generated.stderr.is_empty());
    let outcome = comparison
        .compare(&consumer, |_, command| {
            command.arg(&wire);
        })
        .expect("PHP and Phera stop before expanding the stream");
    let output = String::from_utf8(outcome.stdout).expect("inflate diagnostics are ASCII");
    assert!(output.contains("gzdecode(): insufficient memory"));
    assert!(output.contains("zlib_decode(): insufficient memory"));
}
