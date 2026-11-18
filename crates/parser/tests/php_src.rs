//! Parses the `--FILE--` section of php-src `.phpt` tests with Phera and
//! checks every file real PHP accepts against `token_get_all(..., TOKEN_PARSE)`.
//!
//! Set `PHERA_PARSE_SCAN=<dir>` to scan every `.phpt` under a directory (such
//! as a php-src checkout). Without it, the repository's selected tests
//! (`tests/upstream` + `tests/fixtures`) run. Needs a `php` binary (override
//! with `PHERA_PHP=/path/to/php`); skipped without one.
//!
//! Files Phera rejects on purpose (lexer `Unsupported`: backticks, `${...}`,
//! `(real)`, short tags), non-UTF-8 sources and nesting beyond Phera's limit
//! are counted as "not relevant".

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use phera_phpt::{Phpt, display_path, filters_from_args, matches_filters};

/// One test source: display name, temp file for PHP, and the code itself.
struct Source {
    name: String,
    file: PathBuf,
    code: String,
}

fn main() -> ExitCode {
    let Some(php) = phera_extension_api::testing::oracle::find_php() else {
        println!("skipping oracle: PHP not found (set PHERA_PHP=/path/to/php)");
        return ExitCode::SUCCESS;
    };

    let scan_dir = std::env::var_os("PHERA_PARSE_SCAN")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    let tests = match &scan_dir {
        Some(dir) => phera_phpt::find_phpt_files(dir),
        None => match phera_phpt::selected_tests() {
            Ok(tests) => tests,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        },
    };

    let filters = filters_from_args();
    let tests: Vec<_> = tests
        .into_iter()
        .filter(|t| matches_filters(t, &filters))
        .collect();

    let name = |path: &Path| match &scan_dir {
        Some(dir) => path.strip_prefix(dir).unwrap_or(path).display().to_string(),
        None => display_path(path),
    };

    // Write every `--FILE--` section out so PHP can check them in one process.
    let work_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("phpt-parser");
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).expect("cannot create work directory");

    let mut sources: Vec<Source> = Vec::new();
    let mut not_relevant: BTreeMap<String, usize> = BTreeMap::new();

    for (index, path) in tests.iter().enumerate() {
        let Ok(phpt) = Phpt::parse_file(path) else {
            continue;
        };
        let Some(code) = phpt.section("FILE") else {
            continue;
        };

        // Sources in other encodings aren't valid UTF-8, and Phera only takes
        // `&str`. `Phpt` reads lossily, so such code shows up with replacement
        // characters.
        if code.contains('\u{FFFD}') {
            *not_relevant
                .entry("source is not UTF-8".into())
                .or_default() += 1;
            continue;
        }

        let file = work_dir.join(format!("{index}.php"));
        fs::write(&file, code).expect("cannot write source file");
        sources.push(Source {
            name: name(path),
            file,
            code: code.to_string(),
        });
    }

    let php_verdicts = match check_with_php(&php, &sources) {
        Ok(verdicts) => verdicts,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    let parsed: Vec<_> = sources
        .iter()
        .map(|source| phera_parser::parse(&source.code).map(|_| ()))
        .collect();

    let mut passed = 0;
    let mut both_reject = 0;
    let mut phera_accepts_php_rejects = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    for (source, (php_ok, parse_result)) in
        sources.iter().zip(php_verdicts.iter().zip(parsed.iter()))
    {
        match (php_ok, parse_result) {
            (true, Ok(())) => passed += 1,
            (true, Err(error)) => {
                if error.is_unsupported_lex() {
                    *not_relevant
                        .entry("lexer rejects it on purpose (Unsupported)".into())
                        .or_default() += 1;
                } else if error.message.contains("nesting is too deep") {
                    // PHP's own stack-limit stress tests.
                    *not_relevant
                        .entry("nests deeper than Phera's limit".into())
                        .or_default() += 1;
                } else {
                    failures.push((source.name.clone(), format!("{error}")));
                }
            }
            (false, Err(_)) => both_reject += 1,
            (false, Ok(())) => phera_accepts_php_rejects += 1,
        }
    }

    // PHERA_PARSE_FAILURES=<n> prints more than the first 25 failures.
    let limit: usize = std::env::var("PHERA_PARSE_FAILURES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);

    println!();
    for (name, details) in failures.iter().take(limit) {
        println!("FAIL  {name}\n      {details}");
    }
    if failures.len() > limit {
        println!("... and {} more failures", failures.len() - limit);
    }

    let skipped: usize = not_relevant.values().sum();
    if skipped > 0 {
        println!("\nnot relevant:");
        for (reason, count) in &not_relevant {
            println!("  {count:>6}  {reason}");
        }
    }

    println!(
        "\nparser result: {passed} passed; {} failed; {} both reject; {} Phera accepts but PHP rejects; {skipped} not relevant; {} total",
        failures.len(),
        both_reject,
        phera_accepts_php_rejects,
        sources.len(),
    );

    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Asks PHP, in one process, which of the sources its parser accepts.
fn check_with_php(php: &Path, sources: &[Source]) -> Result<Vec<bool>, String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parse_check.php");

    let mut child = Command::new(php)
        .arg("-n")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start `{php:?}`: {e}"))?;

    let mut input = String::new();
    for source in sources {
        input.push_str(&source.file.to_string_lossy());
        input.push('\n');
    }

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));

    let output = child
        .wait_with_output()
        .map_err(|e| format!("cannot run `{php:?}`: {e}"))?;
    let _ = writer.join();

    if !output.status.success() {
        return Err(format!("`{php:?}` failed with {}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut verdicts = Vec::new();
    for line in stdout.lines() {
        match line {
            "OK" => verdicts.push(true),
            "ERR" => verdicts.push(false),
            other => return Err(format!("unexpected output from PHP: {other:?}")),
        }
    }

    if verdicts.len() != sources.len() {
        return Err(format!(
            "PHP returned {} verdicts for {} sources",
            verdicts.len(),
            sources.len()
        ));
    }

    Ok(verdicts)
}
