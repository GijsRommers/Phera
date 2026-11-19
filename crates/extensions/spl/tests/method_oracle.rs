//! Compare native method calls with PHP 8.5 using the same generated inputs.
//! Set PHERA_BIN to the CLI produced by the workspace build.

mod cases;

use std::fmt::Write;

use phera_extension_api::testing::{
    oracle,
    scripts::{Comparison, Runtime},
};

#[test]
fn methods_match_php() {
    let Some(comparison) = Comparison::from_environment("spl-methods") else {
        return;
    };
    let mut failures = Vec::new();
    let mut compared = 0;
    for (group, cases) in cases::matrix() {
        let path = comparison.artifacts.join(format!("{group}.php"));
        let mut script = "<?php\n".to_owned();
        let (definitions, _) = oracle::PHP_PREAMBLE.split_once("function run(").unwrap();
        let (before, after) = definitions.split_once("bin2hex(pack('E', $f))").unwrap();
        script.push_str(before);
        script.push_str("sprintf('%.17e', $f)");
        script.push_str(after);
        script.push_str(RUN);
        for (label, setup, expression) in &cases {
            writeln!(
                script,
                "run('{label}', function () {{ {setup} return {expression}; }});"
            )
            .unwrap();
        }
        std::fs::write(&path, script).unwrap();
        let php = comparison.command(Runtime::Php, &path).output().unwrap();
        let phera = comparison.command(Runtime::Phera, &path).output().unwrap();
        let result = comparison.check(group, &php, &phera);
        let php_lines = output_lines(&php.stdout);
        let phera_lines = output_lines(&phera.stdout);
        if php.status.success()
            && phera.status.success()
            && php_lines.len() == cases.len()
            && phera_lines.len() == cases.len()
        {
            let mut failed = 0;
            for (expected, actual) in php_lines.into_iter().zip(phera_lines) {
                if expected != actual {
                    let expected = String::from_utf8_lossy(expected);
                    let actual = String::from_utf8_lossy(actual);
                    failures.push(format!("{group}\n  PHP   {expected:?}\n  Phera {actual:?}"));
                    failed += 1;
                }
            }
            eprintln!("{group}: {} compared, {failed} mismatches", cases.len());
            if failed == 0
                && let Err(error) = result
            {
                failures.push(error);
            }
            compared += cases.len();
        } else {
            failures.push(format!(
                "{group}: compared {} / {} lines across {} cases (PHP {}, Phera {})",
                php_lines.len(),
                phera_lines.len(),
                cases.len(),
                php.status,
                phera.status,
            ));
        }
    }
    eprintln!("{compared} calls compared");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn output_lines(output: &[u8]) -> Vec<&[u8]> {
    output
        .strip_suffix(b"\n")
        .unwrap_or(output)
        .split(|&byte| byte == b'\n')
        .collect()
}

const RUN: &str = r#"
function run($label, $call) {
    $GLOBALS['ERRBUF'] = [];
    try {
        $value = enc($call());
    } catch (Throwable $error) {
        $value = 'E:' . get_class($error) . ':' . $error->getMessage();
    }
    echo $label, '=', $value;
    foreach ($GLOBALS['ERRBUF'] as $diagnostic) {
        echo '|', str_replace("\n", '\n', $diagnostic);
    }
    echo "\n";
}
"#;
