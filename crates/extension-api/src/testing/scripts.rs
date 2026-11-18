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

use super::oracle::find_php;

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
        let php = find_php().expect("PHP 8.5 is required as the oracle");
        let artifacts = std::env::temp_dir().join("phera-oracle").join(name);
        std::fs::create_dir_all(&artifacts).expect("create the artifact directory");
        Some(Self {
            php,
            phera: PathBuf::from(phera),
            artifacts,
        })
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
            Some("PHP failed")
        } else if !phera.status.success() {
            Some("Phera failed")
        } else if php.stdout != phera.stdout {
            Some("stdout differs")
        } else if php.stderr != phera.stderr {
            Some("stderr differs")
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

    /// Compares every script and panics with the list of those that differ.
    /// Returns what PHP printed for each, for tests that count cases.
    pub fn compare_all(
        &self,
        scripts: &[PathBuf],
        mut configure: impl FnMut(&Path, Runtime, &mut Command),
    ) -> Vec<Matched> {
        let mut matched = Vec::new();
        let mut failures = Vec::new();
        for script in scripts {
            match self.compare(script, |runtime, command| {
                configure(script, runtime, command)
            }) {
                Ok(outcome) => matched.push(outcome),
                Err(failure) => failures.push(failure),
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} scripts differ from PHP; outputs in {}\n{}",
            failures.len(),
            scripts.len(),
            self.artifacts.display(),
            failures.join("\n")
        );
        eprintln!("{} scripts match PHP", scripts.len());
        matched
    }
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
