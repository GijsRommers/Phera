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

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
const SUPPORTED_SECTIONS: &[&str] = &["FILE", "EXPECT", "EXPECTF", "ARGS", "SKIPIF", "XFAIL"];

enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
    /// Failed, but the test is marked `--XFAIL--`.
    ExpectedFail,
}

fn main() -> ExitCode {
    let filters = filters_from_args();
    let binary = binary();

    let tests = match phera_phpt::selected_tests() {
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
        "running {} phpt tests with `{}`\n",
        tests.len(),
        binary.join(" ")
    );

    let (mut passed, mut failed, mut skipped, mut xfailed) = (0, 0, 0, 0);
    let mut failures = Vec::new();

    for path in &tests {
        let name = display_path(path);

        match run_test(&binary, path) {
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
            Outcome::ExpectedFail => {
                xfailed += 1;
                println!("XFAIL {name}");
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
fn binary() -> Vec<String> {
    match std::env::var("PHERA_PHPT_BIN") {
        Ok(value) if !value.trim().is_empty() => {
            value.split_whitespace().map(String::from).collect()
        }
        _ => vec![env!("CARGO_BIN_EXE_phera").to_string()],
    }
}

fn run_test(binary: &[String], path: &Path) -> Outcome {
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

    let Some(file) = phpt.section("FILE") else {
        return Outcome::Skip("no --FILE-- section".into());
    };

    let args: Vec<&str> = phpt
        .section("ARGS")
        .map(|a| a.split_whitespace().collect())
        .unwrap_or_default();

    if let Some(skipif) = phpt.section("SKIPIF") {
        match run_php(binary, path, "skip.php", skipif, &[]) {
            Ok(run)
                if run
                    .stdout
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("skip") =>
            {
                return Outcome::Skip(run.stdout.trim().to_string());
            }
            Ok(_) => {}
            Err(error) => return Outcome::Fail(format!("--SKIPIF-- failed: {error}")),
        }
    }

    let outcome = match run_php(binary, path, "php", file, &args) {
        Ok(run) => check_output(&phpt, &run),
        Err(error) => Outcome::Fail(error),
    };

    match outcome {
        Outcome::Fail(_) if phpt.section("XFAIL").is_some() => Outcome::ExpectedFail,
        outcome => outcome,
    }
}

fn check_output(phpt: &Phpt, run: &Run) -> Outcome {
    let actual = run.stdout.replace("\r\n", "\n");

    let matched = if let Some(expect) = phpt.section("EXPECT") {
        expect.replace("\r\n", "\n").trim() == actual.trim()
    } else if let Some(pattern) = phpt.section("EXPECTF") {
        match expectf_matches(pattern, &actual) {
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
        .section("EXPECT")
        .or(phpt.section("EXPECTF"))
        .unwrap_or_default();

    let mut details = format!(
        "expected:\n{}\n\nactual:\n{}",
        indent(expected.trim()),
        indent(actual.trim())
    );

    if !run.stderr.trim().is_empty() {
        details.push_str(&format!("\n\nstderr:\n{}", indent(run.stderr.trim())));
    }
    if let Some(status) = &run.status {
        details.push_str(&format!("\n\nexit status: {status}"));
    }

    Outcome::Fail(details)
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
    stdout: String,
    stderr: String,
    /// Set when the process didn't exit successfully.
    status: Option<String>,
}

/// Writes `code` next to the test (so relative includes and `__DIR__` work, as
/// with `run-tests.php`), runs it, and removes it again.
fn run_php(
    binary: &[String],
    test: &Path,
    extension: &str,
    code: &str,
    args: &[&str],
) -> Result<Run, String> {
    let script = TempScript::new(test.with_extension(extension), code)
        .map_err(|e| format!("cannot write script: {e}"))?;

    let dir = test.parent().unwrap_or(Path::new("."));
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

fn read_in_background(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// A script file that is deleted when dropped, even if the runner panics.
struct TempScript(PathBuf);

impl TempScript {
    fn new(path: PathBuf, code: &str) -> std::io::Result<Self> {
        fs::write(&path, code)?;
        Ok(Self(path))
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
