//! Very long and very deeply nested sources, compared with real PHP.
//!
//! `tests/long-sources/generate.php` builds each shape at a given size: a
//! 200,000-term `+` chain, an array literal of 100,000 items, a thousand
//! nested parentheses. Every case below runs the generated file through PHP
//! and through Phera and compares what they print. The sizes are the ones
//! that went wrong before Phera handled such sources (a stack overflow, the
//! register limit, a nesting error, minutes of compiling), kept small enough
//! for a debug build to finish each in a few seconds.
//!
//! PHP's resource limits are not behaviour: where PHP stops at its
//! compiler's stack limit ("Maximum call stack size ... reached during
//! compilation") it runs again with a larger stack, and its memory limit is
//! off. Nesting past the limits is an error in both: PHP's "memory
//! exhausted", which Phera gives too where it counts the same, or Phera's own
//! nesting limit (see `crates/parser/src/nesting.rs`).
//!
//! `PHERA_LONG_SOURCES_BIN` runs the cases against another Phera binary, such
//! as a build of an older commit.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use phera_extension_api::testing::oracle::find_php;

/// What a case must do.
#[derive(Clone, Copy)]
enum Expect {
    /// Print exactly what PHP prints.
    SameAsPhp,
    /// Stop with a parse error, as PHP does.
    NestingError,
}

use Expect::{NestingError, SameAsPhp};

const CASES: &[(&str, usize, Expect)] = &[
    // Left-associative chains. Under an assignment, the parser used to copy
    // the right side, recursively: 200,000 terms overflowed its stack.
    ("add", 200_000, SameAsPhp),
    ("add-vars", 200_000, SameAsPhp),
    ("subtract", 200_000, SameAsPhp),
    ("mixed-arithmetic", 100_000, SameAsPhp),
    ("concat", 200_000, SameAsPhp),
    ("concat-vars", 200_000, SameAsPhp),
    ("keyword-or", 200_000, SameAsPhp),
    ("logical-and", 100_000, SameAsPhp),
    ("logical-or", 100_000, SameAsPhp),
    ("bitwise-or", 100_000, SameAsPhp),
    ("bitwise-xor", 100_000, SameAsPhp),
    ("comparisons", 50_000, SameAsPhp),
    ("pipe", 100_000, SameAsPhp),
    ("instanceof", 50_000, SameAsPhp),
    // Took a minute at 10,000: every step kept its temporaries.
    ("short-ternary", 10_000, SameAsPhp),
    // Member and call chains ran out of registers at 100,000 steps.
    ("method-chain", 100_000, SameAsPhp),
    ("nullsafe-chain", 100_000, SameAsPhp),
    ("property-chain", 100_000, SameAsPhp),
    ("static-chain", 100_000, SameAsPhp),
    ("subscript-chain", 100_000, SameAsPhp),
    ("call-chain", 100_000, SameAsPhp),
    // Wide lists ran out of registers at 100,000 items, or took minutes.
    ("array-wide", 100_000, SameAsPhp),
    ("array-wide-keyed", 100_000, SameAsPhp),
    ("array-wide-spread", 100_000, SameAsPhp),
    ("array-wide-nested", 50_000, SameAsPhp),
    ("list-nested-wide", 10_000, SameAsPhp),
    ("arguments-wide", 100_000, SameAsPhp),
    ("interpolation-wide", 50_000, SameAsPhp),
    ("heredoc-wide", 50_000, SameAsPhp),
    ("match-arms", 50_000, SameAsPhp),
    ("switch-cases", 50_000, SameAsPhp),
    ("elseif-ladder", 100_000, SameAsPhp),
    // goto: labels and jumps resolve through hash tables, in one pass.
    ("labels-wide", 100_000, SameAsPhp),
    ("gotos-wide", 100_000, SameAsPhp),
    ("elseif-goto-ladder", 100_000, SameAsPhp),
    ("statements", 100_000, SameAsPhp),
    ("echo-list", 100_000, SameAsPhp),
    ("isset-list", 100_000, SameAsPhp),
    ("unset-list", 100_000, SameAsPhp),
    // Each name is a variable, and a function has at most 65,534 of them.
    ("list-wide", 10_000, SameAsPhp),
    ("global-list", 10_000, SameAsPhp),
    ("closure-use-list", 10_000, SameAsPhp),
    ("parameters-wide", 10_000, SameAsPhp),
    ("functions", 20_000, SameAsPhp),
    ("class-constants", 10_000, SameAsPhp),
    // Nesting that PHP accepts at a thousand levels was a nesting error.
    ("coalesce", 1_000, SameAsPhp),
    ("power", 1_000, SameAsPhp),
    ("assign-chain", 1_000, SameAsPhp),
    ("concat-assign-chain", 1_000, SameAsPhp),
    ("ternary-middle", 1_000, SameAsPhp),
    ("ternary-parenthesised", 1_000, SameAsPhp),
    ("right-nested-add", 1_000, SameAsPhp),
    ("not-run", 1_000, SameAsPhp),
    ("minus-run", 1_000, SameAsPhp),
    ("bitwise-not-run", 1_000, SameAsPhp),
    ("cast-run", 1_000, SameAsPhp),
    ("silence-run", 1_000, SameAsPhp),
    ("parentheses", 1_000, SameAsPhp),
    ("clone-run", 1_000, SameAsPhp),
    ("array-nested", 1_000, SameAsPhp),
    ("list-nested-deep", 1_000, SameAsPhp),
    ("calls-nested", 1_000, SameAsPhp),
    ("new-nested", 1_000, SameAsPhp),
    ("new-chain", 1_000, SameAsPhp),
    ("else-if-ladder", 1_000, SameAsPhp),
    ("blocks-nested", 1_000, SameAsPhp),
    ("if-nested", 1_000, SameAsPhp),
    ("while-nested", 1_000, SameAsPhp),
    ("foreach-nested", 1_000, SameAsPhp),
    // PHP's parser stack is full here already, and Phera counts the same.
    ("closures-nested", 1_000, SameAsPhp),
    ("arrow-nested", 1_000, SameAsPhp),
    ("static-arrow-nested", 1_000, SameAsPhp),
    ("for-nested", 1_000, SameAsPhp),
    // Past both limits.
    ("parentheses", 10_000, NestingError),
    ("array-nested", 10_000, NestingError),
    ("calls-nested", 10_000, NestingError),
    ("if-nested", 10_000, NestingError),
    ("closures-nested", 10_000, NestingError),
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn phera() -> PathBuf {
    std::env::var_os("PHERA_LONG_SOURCES_BIN")
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_phera")), PathBuf::from)
}

/// Exit code and output, with the script's path replaced by `%s`.
fn run(mut command: Command, script: &Path) -> (Option<i32>, String) {
    let output = command.output().expect("run engine");
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    (
        output.status.code(),
        text.replace(&*script.to_string_lossy(), "%s"),
    )
}

fn run_php(php: &Path, script: &Path) -> (Option<i32>, String) {
    let mut command = Command::new(php);
    command.args(["-n", "-d", "memory_limit=-1"]).arg(script);
    let result = run(command, script);
    if !result.1.contains("Maximum call stack size") {
        return result;
    }
    // A larger stack, which PHP then finds by itself.
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("ulimit -s 65520 2>/dev/null; exec \"$0\" -n -d memory_limit=-1 \"$1\"")
        .arg(php)
        .arg(script);
    run(command, script)
}

fn check(php: &Path, directory: &Path, shape: &str, size: usize, expect: Expect) -> Option<String> {
    let script = directory.join(format!("{shape}-{size}.php"));
    let source = Command::new(php)
        .args(["-n", "-d", "memory_limit=-1"])
        .arg(repository().join("tests/long-sources/generate.php"))
        .args([shape, &size.to_string()])
        .output()
        .expect("run the generator");
    std::fs::write(&script, source.stdout).expect("write the script");

    let (php_code, php_output) = run_php(php, &script);
    let mut command = Command::new(phera());
    command.arg(&script);
    let (phera_code, phera_output) = run(command, &script);
    let failure = match expect {
        SameAsPhp if php_output.contains("Maximum call stack size") => {
            // This machine cannot give PHP a large enough stack: Phera
            // must still run the file.
            (phera_code != Some(0)).then(|| format!("Phera failed: {phera_output}"))
        }
        SameAsPhp => (php_code != phera_code || php_output != phera_output).then(|| {
            format!("PHP ({php_code:?}): {php_output}\nPhera ({phera_code:?}): {phera_output}")
        }),
        NestingError => (!php_output.contains("Parse error: memory exhausted")
            || !phera_output.contains("Parse error: ")
            || phera_code != Some(255))
        .then(|| format!("PHP: {php_output}\nPhera ({phera_code:?}): {phera_output}")),
    };
    let _ = std::fs::remove_file(&script);
    failure.map(|failure| format!("{shape} at {size}:\n{failure}"))
}

#[test]
fn long_and_deep_sources_behave_as_in_php() {
    let Some(php) = find_php() else {
        eprintln!("skipped: no php binary");
        return;
    };
    let directory = std::env::temp_dir().join(format!("phera-long-sources-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create the scratch directory");
    // As PHP reports it: on macOS the temporary directory is a symbolic link.
    let directory = directory
        .canonicalize()
        .expect("resolve the scratch directory");

    let next = Mutex::new(CASES.iter());
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(2, |count| count.get().min(8));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(&(shape, size, expect)) =
                    next.lock().map(|mut cases| cases.next()).ok().flatten()
                {
                    if let Some(failure) = check(&php, &directory, shape, size, expect) {
                        failures.lock().expect("failures").push(failure);
                    }
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&directory);

    let failures = failures.into_inner().expect("failures");
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n\n")
    );
}
