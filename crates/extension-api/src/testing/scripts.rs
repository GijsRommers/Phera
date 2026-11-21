//! Comparing whole PHP scripts: the same file runs through real PHP and
//! through the `phera` executable, and both must succeed and print the same
//! bytes, on stdout and on stderr.
//!
//! ```no_run
//! use phera_extension_api::testing::scripts::{Comparison, php_scripts};
//!
//! let Some(comparison) = Comparison::from_environment("example") else {
//!     return; // PHERA_BIN is not set
//! };
//! let scripts = php_scripts("tests/scripts".as_ref(), "");
//! comparison.compare_all(&scripts, |_script, _runtime, _command| {});
//! ```
//!
//! The scripts need a release build of Phera, named by `PHERA_BIN`; CI's end
//! to end job sets it. Both outputs of every run are kept in a directory of
//! the system's temporary directory, named in the failure message.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::oracle::{find_php, oracle_status, supports_php_since};

/// Which side of a comparison a command runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Php,
    Phera,
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::Php => "php",
            Runtime::Phera => "phera",
        }
    }
}

pub struct Comparison {
    php: PathBuf,
    phera: PathBuf,
    /// Where both outputs of every run are saved.
    pub artifacts: PathBuf,
}

/// What real PHP printed, once a script matched.
pub struct Matched {
    pub stdout: Vec<u8>,
}

impl Comparison {
    /// `None`, after a notice, when `PHERA_BIN` is not set. `name` keeps the
    /// artifacts of different test files apart.
    pub fn from_environment(name: &str) -> Option<Self> {
        let Some(phera) = std::env::var_os("PHERA_BIN") else {
            eprintln!("SKIP: set PHERA_BIN to a release phera executable to compare scripts");
            return None;
        };
        Some(Self::for_binary(name, Path::new(&phera)))
    }

    /// Compares scripts using a known executable, for tests that build their own CLI.
    pub fn for_binary(name: &str, phera: &Path) -> Self {
        let php = find_php().expect("PHP 8.5 is required as the oracle");
        let artifacts = std::env::temp_dir().join("phera-oracle").join(name);
        std::fs::create_dir_all(&artifacts).expect("create the artifact directory");
        Self {
            php,
            phera: phera.to_path_buf(),
            artifacts,
        }
    }

    /// The command that runs `script` on `runtime`. Real PHP runs without an
    /// ini file and shows errors on stdout, as Phera does.
    pub fn command(&self, runtime: Runtime, script: &Path) -> Command {
        let mut command = match runtime {
            Runtime::Php => {
                let mut command = Command::new(&self.php);
                command.args(["-n", "-d", "display_errors=1", "-d", "log_errors=0"]);
                command
            }
            Runtime::Phera => Command::new(&self.phera),
        };
        command.arg(script);
        command
    }

    /// A fresh, empty directory for one run of `script` on `runtime`, for
    /// scripts that write files: SQLite's take it as their working directory
    /// and first argument, so the two runs do not share a database.
    pub fn scratch_directory(&self, script: &Path, runtime: Runtime) -> PathBuf {
        let directory = self
            .artifacts
            .join(format!("{}-{}", stem(script), runtime.label()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create a scratch directory");
        directory
    }

    /// Checks two runs of the script `label` against each other and saves
    /// their outputs as `<label>.php.out`, `<label>.phera.err` and so on.
    pub fn check(&self, label: &str, php: &Output, phera: &Output) -> Result<Matched, String> {
        for (runtime, output) in [(Runtime::Php, php), (Runtime::Phera, phera)] {
            for (stream, bytes) in [("out", &output.stdout), ("err", &output.stderr)] {
                let file = self
                    .artifacts
                    .join(format!("{label}.{}.{stream}", runtime.label()));
                std::fs::write(file, bytes).expect("save the script output");
            }
        }
        let problem = if !php.status.success() {
            Some("PHP failed".to_owned())
        } else if !phera.status.success() {
            Some("Phera failed".to_owned())
        } else if php.stdout != phera.stdout {
            Some(format!(
                "stdout differs\n{}",
                first_difference(&php.stdout, &phera.stdout)
            ))
        } else if php.stderr != phera.stderr {
            Some(format!(
                "stderr differs\n{}",
                first_difference(&php.stderr, &phera.stderr)
            ))
        } else {
            None
        };
        match problem {
            Some(problem) => Err(format!("{label}: {problem}")),
            None => Ok(Matched {
                stdout: php.stdout.clone(),
            }),
        }
    }

    /// Runs `script` on both sides, after `configure` has added what the
    /// test needs (arguments, environment, a working directory), and checks
    /// the outputs.
    pub fn compare(
        &self,
        script: &Path,
        mut configure: impl FnMut(Runtime, &mut Command),
    ) -> Result<Matched, String> {
        let mut run = |runtime| {
            let mut command = self.command(runtime, script);
            configure(runtime, &mut command);
            command.output().expect("start the script")
        };
        let php = run(Runtime::Php);
        let phera = run(Runtime::Phera);
        self.check(&stem(script), &php, &phera)
    }

    /// Checks a table of individually tagged calls and keeps every mismatch.
    pub fn check_matrix(&self, script: &str, expected_calls: usize) {
        use std::collections::BTreeMap;
        use std::fmt::Write;

        fn cases(data: &[u8]) -> BTreeMap<String, String> {
            let mut cases = BTreeMap::new();
            for line in String::from_utf8_lossy(data).lines() {
                if let Some((label, value)) = line.split_once('=') {
                    assert!(
                        cases.insert(label.into(), value.into()).is_none(),
                        "duplicate case label {label}"
                    );
                }
            }
            cases
        }

        let path = self.artifacts.join("matrix.php");
        std::fs::write(&path, script).expect("write comparison matrix");
        let php = self
            .command(Runtime::Php, &path)
            .output()
            .expect("PHP oracle");
        let phera = self.command(Runtime::Phera, &path).output().expect("Phera");
        let outcome = self.check("matrix", &php, &phera);
        let php_cases = cases(&php.stdout);
        let phera_cases = cases(&phera.stdout);
        let mut report = String::new();
        let mut mismatches = 0;
        for label in php_cases.keys().chain(
            phera_cases
                .keys()
                .filter(|key| !php_cases.contains_key(*key)),
        ) {
            let left = php_cases.get(label);
            let right = phera_cases.get(label);
            if left != right {
                mismatches += 1;
                writeln!(report, "{label}\n  PHP: {left:?}\n  Phera: {right:?}").unwrap();
            }
        }
        let report_path = self.artifacts.join("matrix.diff");
        std::fs::write(&report_path, report).expect("write complete mismatch report");
        assert!(
            outcome.is_ok()
                && php_cases.len() == expected_calls
                && phera_cases.len() == expected_calls,
            "{mismatches} mismatches across {} PHP and {} Phera cases (expected {expected_calls}), {}. Details in {}",
            php_cases.len(),
            phera_cases.len(),
            outcome.err().unwrap_or_default(),
            report_path.display(),
        );
        println!("{expected_calls} matched calls");
    }

    /// Compares every script and panics with the list of those that differ.
    /// Returns what PHP printed for each, for tests that count cases. A script
    /// annotated `// PHERA_ORACLE_PHP_SINCE: 80513` requires PHP 8.5.13 or newer.
    pub fn compare_all(
        &self,
        scripts: &[PathBuf],
        mut configure: impl FnMut(&Path, Runtime, &mut Command),
    ) -> Vec<Matched> {
        let mut matched = Vec::new();
        let mut failures = Vec::new();
        let mut skipped = 0;
        let mut versioned = 0;
        for script in scripts {
            if let Some(minimum) = minimum_php(script) {
                versioned += 1;
                if !supports_php_since(&stem(script), minimum) {
                    skipped += 1;
                    continue;
                }
            }
            match self.compare(script, |runtime, command| {
                configure(script, runtime, command)
            }) {
                Ok(outcome) => matched.push(outcome),
                Err(failure) => failures.push(failure),
            }
        }
        if versioned == 0 {
            eprintln!("{} scripts match PHP", matched.len());
        } else {
            oracle_status(format_args!(
                "{}: {} scripts match PHP, {skipped}/{versioned} versioned scripts skipped",
                self.artifacts.display(),
                matched.len()
            ));
        }
        assert!(
            failures.is_empty(),
            "{} of {} scripts differ from PHP; outputs in {}\n{}",
            failures.len(),
            scripts.len() - skipped,
            self.artifacts.display(),
            failures.join("\n")
        );
        matched
    }
}

/// Where two outputs first differ, with a few lines of each from there, so
/// that a failure on another machine (the Linux runner) can be read in its
/// log without the saved files.
///
/// ```text
///   line 12
///   PHP:   string(21) "Name or service not known"
///   Phera: string(18) "nodename not known"
/// ```
fn first_difference(php: &[u8], phera: &[u8]) -> String {
    const LINES_SHOWN: usize = 4;
    let php = String::from_utf8_lossy(php);
    let phera = String::from_utf8_lossy(phera);
    let same = php
        .lines()
        .zip(phera.lines())
        .take_while(|(left, right)| left == right)
        .count();
    let shown = |text: &str| -> String {
        let lines: Vec<&str> = text.lines().skip(same).take(LINES_SHOWN).collect();
        if lines.is_empty() {
            "(the output ends here)".to_owned()
        } else {
            lines.join("\n         ")
        }
    };
    format!(
        "  line {}\n  PHP:   {}\n  Phera: {}",
        same + 1,
        shown(&php),
        shown(&phera)
    )
}

fn minimum_php(script: &Path) -> Option<u32> {
    let source = std::fs::read(script).expect("read oracle script");
    String::from_utf8_lossy(&source).lines().find_map(|line| {
        line.strip_prefix("// PHERA_ORACLE_PHP_SINCE:")
            .map(|version| {
                version
                    .trim()
                    .parse()
                    .expect("numeric PHP version requirement")
            })
    })
}

/// The `.php` files in `directory` whose names start with `prefix`, sorted.
pub fn php_scripts(directory: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("read the script directory")
        .map(|entry| entry.expect("read a script directory entry").path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "php")
                && stem(path).starts_with(prefix)
        })
        .collect();
    scripts.sort();
    scripts
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::first_difference;

    #[test]
    fn the_first_differing_line_is_shown_with_what_follows() {
        let report = first_difference(b"a\nb\nc\nd\n", b"a\nB\nc\n");
        assert_eq!(
            report,
            "  line 2\n  PHP:   b\n         c\n         d\n  Phera: B\n         c"
        );
    }

    #[test]
    fn an_output_that_ends_early_says_so() {
        let report = first_difference(b"a\nb\n", b"a\n");
        assert_eq!(
            report,
            "  line 2\n  PHP:   b\n  Phera: (the output ends here)"
        );
    }
}
