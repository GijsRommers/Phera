//! End-to-end `.phpt` runner: executes each selected test with the `phera`
//! binary and compares its output with the test's expectation.
//!
//! Run with `make phpt` or `cargo test -p phera-cli --test phpt -- [FILTER...]`.
//! Set `PHERA_PHPT_BIN` to run the tests with another binary instead. To check
//! the tests themselves against real PHP, use the settings `run-tests.php` uses:
//!
//! ```sh
//! PHERA_PHPT_BIN="php -n -d fatal_error_backtraces=0 -d display_errors=1" make phpt
//! ```

mod phpt_support;

use phpt_support::expected_outcome;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use phera_extension_api::{LanguageLevel, testing::oracle::test_level};
use phera_phpt::{Phpt, display_path, expectf_matches, filters_from_args, matches_filters};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Sections that don't affect how a test runs.
const IGNORED_SECTIONS: &[&str] = &[
    "TEST",
    "DESCRIPTION",
    "CREDITS",
    "CONFLICTS",
    "XLEAK",
    "WHITESPACE_SENSITIVE",
    "CLEAN",
];

/// Sections the runner understands.
const SUPPORTED_SECTIONS: &[&str] = &[
    "FILE",
    "EXPECT",
    "EXPECTF",
    "ARGS",
    "SKIPIF",
    "EXTENSIONS",
    "XFAIL",
];

enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
    /// Failed, but the test is marked `--XFAIL--`.
    ExpectedFail(String),
}

fn main() -> ExitCode {
    let filters = filters_from_args();
    let level = test_level();
    let binary = binary(level);

    let tests = match phera_phpt::selected_tests_for(level) {
        Ok(tests) => tests,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    let tests: Vec<_> = tests
        .into_iter()
        .filter(|t| matches_filters(t, &filters))
        .collect();

    println!(
        "running {} phpt tests at PHP {} with `{}`\n",
        tests.len(),
        level.name(),
        binary.join(" ")
    );

    let (mut passed, mut failed, mut skipped, mut xfailed) = (0, 0, 0, 0);
    let mut failures = Vec::new();

    for path in &tests {
        let name = display_path(path);

        match expected_outcome(run_test(&binary, path, level), &name, level) {
            Outcome::Pass => {
                passed += 1;
                println!("PASS  {name}");
            }
            Outcome::Fail(details) => {
                failed += 1;
                println!("FAIL  {name}");
                failures.push((name, details));
            }
            Outcome::Skip(reason) => {
                skipped += 1;
                println!("SKIP  {name} ({reason})");
            }
            Outcome::ExpectedFail(reason) => {
                xfailed += 1;
                println!("XFAIL {name} ({reason})");
            }
        }
    }

    for (name, details) in &failures {
        println!("\n---- {name} ----\n{details}");
    }

    println!(
        "\nphpt result: {passed} passed; {failed} failed; {skipped} skipped; {xfailed} expected failures"
    );

    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The command used to run PHP files, split into program and leading arguments.
fn binary(level: LanguageLevel) -> Vec<String> {
    match std::env::var("PHERA_PHPT_BIN") {
        Ok(value) if !value.trim().is_empty() => {
            value.split_whitespace().map(String::from).collect()
        }
        _ => [env!("CARGO_BIN_EXE_phera"), "--php", level.name()]
            .map(str::to_owned)
            .to_vec(),
    }
}

fn run_test(binary: &[String], path: &Path, level: LanguageLevel) -> Outcome {
    let phpt = match Phpt::parse_file(path) {
        Ok(phpt) => phpt,
        Err(error) => return Outcome::Fail(format!("cannot read test: {error}")),
    };

    if let Some(section) = phpt
        .section_names()
        .find(|s| !IGNORED_SECTIONS.contains(s) && !SUPPORTED_SECTIONS.contains(s))
    {
        return Outcome::Skip(format!("--{section}-- is not supported"));
    }

    let Some(file) = phpt.section_bytes("FILE") else {
        return Outcome::Skip("no --FILE-- section".into());
    };

    let args: Vec<&str> = phpt
        .section("ARGS")
        .map(|a| a.split_whitespace().collect())
        .unwrap_or_default();

    let probes = [
        phpt.section("EXTENSIONS").map(extension_probe),
        phpt.section("SKIPIF").map(str::to_owned),
    ];
    for probe in probes.into_iter().flatten() {
        match skip_reason(binary, path, &probe) {
            Ok(Some(reason)) => return Outcome::Skip(reason),
            Ok(None) => {}
            Err(error) => return Outcome::Fail(format!("skip check failed: {error}")),
        }
    }

    let revision = match phpt_support::upstream::revision(path, level) {
        Ok(revision) => revision,
        Err(error) => return Outcome::Fail(error),
    };
    let file = match phpt_support::upstream::source(file, revision.as_ref()) {
        Ok(file) => file,
        Err(error) => return Outcome::Fail(error),
    };
    let outcome = match run_php(binary, path, "php", &file, &args) {
        Ok(run) => check_output(revision.as_ref().unwrap_or(&phpt), &run),
        Err(error) => Outcome::Fail(error),
    };

    match outcome {
        Outcome::Fail(_) if phpt.section("XFAIL").is_some() => {
            Outcome::ExpectedFail(phpt.section("XFAIL").unwrap_or_default().trim().to_owned())
        }
        outcome => outcome,
    }
}

/// Runs a script that decides whether the test applies. It says no by
/// printing a line that starts with "skip", as in php-src's `run-tests.php`.
fn skip_reason(binary: &[String], test: &Path, code: &str) -> Result<Option<String>, String> {
    let run = run_php(binary, test, "skip.php", code.as_bytes(), &[])?;
    let text = String::from_utf8_lossy(&run.stdout);
    let output = text.trim();
    let skips = output.to_ascii_lowercase().starts_with("skip");
    Ok(skips.then(|| output.to_owned()))
}

/// A skip script for `--EXTENSIONS--`, which lists one extension per line:
/// the test applies only when the binary has every one of them.
fn extension_probe(section: &str) -> String {
    let names: Vec<String> = section
        .split_whitespace()
        .map(|name| format!("'{}'", name.replace('\'', "")))
        .collect();
    format!(
        "<?php foreach ([{}] as $extension) {{ if (!extension_loaded($extension)) {{ \
         die(\"skip the $extension extension is not loaded\"); }} }}",
        names.join(", ")
    )
}

fn check_output(phpt: &Phpt, run: &Run) -> Outcome {
    let actual = normalize_output(&run.stdout);

    let matched = if let Some(expect) = phpt.section_bytes("EXPECT") {
        let expected = normalize_output(expect);
        trim_output(&expected) == trim_output(&actual)
    } else if let Some(pattern) = phpt.section("EXPECTF") {
        match expectf_matches(pattern, &String::from_utf8_lossy(&actual)) {
            Ok(matched) => matched,
            Err(reason) => return Outcome::Skip(reason),
        }
    } else {
        return Outcome::Skip("no --EXPECT-- or --EXPECTF-- section".into());
    };

    if matched {
        return Outcome::Pass;
    }

    let expected = phpt
        .section_bytes("EXPECT")
        .or(phpt.section_bytes("EXPECTF"))
        .unwrap_or_default();
    let expected = String::from_utf8_lossy(expected);
    let actual = String::from_utf8_lossy(&actual);
    let stderr = String::from_utf8_lossy(&run.stderr);

    let mut details = format!(
        "expected:\n{}\n\nactual:\n{}",
        indent(expected.trim()),
        indent(actual.trim())
    );

    if !stderr.trim().is_empty() {
        details.push_str(&format!("\n\nstderr:\n{}", indent(stderr.trim())));
    }
    if let Some(status) = &run.status {
        details.push_str(&format!("\n\nexit status: {status}"));
    }

    Outcome::Fail(details)
}

fn normalize_output(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    for (index, &byte) in bytes.iter().enumerate() {
        if byte != b'\r' || bytes.get(index + 1) != Some(&b'\n') {
            normalized.push(byte);
        }
    }
    normalized
}

fn trim_output(bytes: &[u8]) -> &[u8] {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim().as_bytes(),
        Err(_) => bytes.trim_ascii(),
    }
}

fn indent(text: &str) -> String {
    if text.is_empty() {
        return "    <empty>".into();
    }

    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Run {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// Set when the process didn't exit successfully.
    status: Option<String>,
}

/// Writes code in a mirrored scratch directory, preserving relative includes
/// without creating generated PHP files in the shared checkout.
fn run_php(
    binary: &[String],
    test: &Path,
    extension: &str,
    code: &[u8],
    args: &[&str],
) -> Result<Run, String> {
    let source_root = phera_phpt::tests_dir()
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let test = test.canonicalize().map_err(|error| error.to_string())?;
    let relative = test
        .strip_prefix(&source_root)
        .map_err(|error| error.to_string())?;
    let destination = scratch_tests()?.join(relative).with_extension(extension);
    let script =
        TempScript::new(destination, code).map_err(|e| format!("cannot write script: {e}"))?;

    let dir = script.0.parent().unwrap_or(Path::new("."));
    let executable = resolve_executable(&binary[0]);

    let mut child = Command::new(&binary[0])
        .args(&binary[1..])
        .arg(&script.0)
        .args(args)
        .current_dir(dir)
        .env("TEST_PHP_EXECUTABLE", &executable)
        .env("TEST_PHP_EXECUTABLE_ESCAPED", shell_quote(&executable))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start `{}`: {e}", binary[0]))?;

    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {}s", TIMEOUT.as_secs()));
            }
            Err(e) => return Err(format!("cannot wait for process: {e}")),
        }
    };

    Ok(Run {
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
        status: (!status.success()).then(|| status.to_string()),
    })
}

/// The absolute, symlink-free path of `program`, looking it up on `PATH` if
/// needed, as tests compare it with PHP's `PHP_BINARY`.
fn resolve_executable(program: &str) -> String {
    let candidates = if program.contains('/') {
        vec![PathBuf::from(program)]
    } else {
        std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join(program))
                    .collect()
            })
            .unwrap_or_default()
    };

    candidates
        .iter()
        .find_map(|path| path.canonicalize().ok())
        .map_or_else(|| program.to_string(), |path| path.display().to_string())
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn read_in_background(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// A script file that is deleted when dropped, even if the runner panics.
struct TempScript(PathBuf);

impl TempScript {
    fn new(path: PathBuf, code: &[u8]) -> std::io::Result<Self> {
        fs::write(&path, code)?;
        Ok(Self(path))
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Mirror support files once, keeping generated scripts out of the checkout.
fn scratch_tests() -> Result<&'static PathBuf, String> {
    static DIRECTORY: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let root = std::env::var_os("PHERA_PHPT_TMP_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("phera-phpt"))
                .join(std::process::id().to_string());
            let source = phera_phpt::tests_dir();
            let mut pending = vec![(source, root.clone())];
            while let Some((source, destination)) = pending.pop() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
                    let entry = entry.map_err(|error| error.to_string())?;
                    let path = entry.path();
                    let destination = destination.join(entry.file_name());
                    if path.is_dir() {
                        pending.push((path, destination));
                    } else if path.extension().is_none_or(|extension| extension != "phpt") {
                        fs::copy(path, destination).map_err(|error| error.to_string())?;
                    }
                }
            }
            Ok(root)
        })
        .as_ref()
        .map_err(Clone::clone)
}
