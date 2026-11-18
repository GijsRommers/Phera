//! Runs one generated PHP file through the `phera` binary with a time limit
//! and a resident-memory limit, and classifies the outcome.
//!
//! macOS has no `ulimit -v`, so the memory limit is enforced by polling the
//! child's RSS with `ps` and killing it when it passes the limit. The child
//! inherits this process's nice level, so starting the fuzzer under
//! `nice -n 15` nices every target too.

use std::fs::{self, File};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// What happened to one executed input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Finished within the limits with a PHP-style status (0 or 255) and no
    /// Rust panic text on stderr.
    Clean(i32),
    /// Killed by a signal (panic-abort, SIGSEGV from a stack overflow, ...).
    Signal(i32),
    /// Exit code that is neither 0 nor 255 and not a signal, with panic text.
    PanicText(i32),
    /// Did not finish within the time limit.
    Timeout,
    /// Resident memory passed the limit; killed.
    MemoryLimit,
    /// Printed more than the output cap; killed.
    OutputBomb,
}

impl Outcome {
    pub fn is_finding(&self) -> bool {
        !matches!(self, Outcome::Clean(_))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Clean(_) => "clean",
            Outcome::Signal(_) => "signal",
            Outcome::PanicText(_) => "panic",
            Outcome::Timeout => "timeout",
            Outcome::MemoryLimit => "memory",
            Outcome::OutputBomb => "output",
        }
    }
}

pub struct Limits {
    pub timeout: Duration,
    /// RSS limit in KiB (as `ps -o rss=` reports). 2 GiB.
    pub mem_kib: u64,
    /// Stdout cap in bytes.
    pub out_cap: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            timeout: Duration::from_secs(5),
            mem_kib: 2 * 1024 * 1024,
            out_cap: 512 * 1024 * 1024,
        }
    }
}

pub struct RunResult {
    pub outcome: Outcome,
    pub elapsed: Duration,
    pub peak_rss_kib: u64,
    pub stdout_bytes: u64,
    pub stderr_tail: String,
    pub command: String,
}

/// `phera file.php` or `phera --dump file.php`.
pub fn run_case(
    phera: &str,
    mode: &str,
    case_path: &Path,
    workdir: &Path,
    limits: &Limits,
) -> RunResult {
    fs::create_dir_all(workdir).expect("create workdir");
    let stdout_path = workdir.join("stdout.bin");
    let stderr_path = workdir.join("stderr.bin");
    let stdout_file = File::create(&stdout_path).expect("create stdout file");
    let stderr_file = File::create(&stderr_path).expect("create stderr file");

    let mut command = Command::new(phera);
    if mode == "dump" {
        command.arg("--dump");
    }
    command
        .arg(case_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    let command_line = format!("{phera} {mode} {}", case_path.display());

    let start = Instant::now();
    let mut child: Child = command.spawn().expect("spawn phera");
    let pid = child.id();

    let mut peak_rss_kib: u64 = 0;
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr_tail = read_tail(&stderr_path, 4096);
                let code = status.code().unwrap_or(-1);
                let outcome = if let Some(signal) = status.signal() {
                    Outcome::Signal(signal)
                } else if looks_like_panic(&stderr_tail) {
                    Outcome::PanicText(code)
                } else {
                    Outcome::Clean(code)
                };
                break outcome;
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Outcome::Signal(-1);
            }
        }
        if start.elapsed() >= limits.timeout {
            let _ = child.kill();
            let _ = child.wait();
            break Outcome::Timeout;
        }
        // Poll RSS and stdout size at ~4 Hz.
        if start.elapsed().as_millis() % 250 < 25 {
            peak_rss_kib = peak_rss_kib.max(rss_kib(pid));
            if peak_rss_kib > limits.mem_kib {
                let _ = child.kill();
                let _ = child.wait();
                break Outcome::MemoryLimit;
            }
            if let Ok(meta) = fs::metadata(&stdout_path) {
                if meta.len() > limits.out_cap {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Outcome::OutputBomb;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    let elapsed = start.elapsed();
    peak_rss_kib = peak_rss_kib.max(rss_kib(pid));
    let stdout_bytes = fs::metadata(&stdout_path).map(|m| m.len()).unwrap_or(0);
    let stderr_tail = read_tail(&stderr_path, 4096);
    RunResult {
        outcome,
        elapsed,
        peak_rss_kib,
        stdout_bytes,
        stderr_tail,
        command: command_line,
    }
}

/// Resident set size of a pid in KiB, via `ps`. 0 when the process is gone.
fn rss_kib(pid: u32) -> u64 {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(pid.to_string())
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

fn read_tail(path: &Path, cap: usize) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut text = String::new();
    let start = len.saturating_sub(cap as u64);
    if file.seek(SeekFrom::Start(start)).is_ok() {
        let _ = file.read_to_string(&mut text);
    }
    text
}

/// Rust panic / abort markers on stderr. With `panic = "abort"` a panic ends
/// in SIGABRT (a signal), but the message is still printed first; some panics
/// happen on a thread whose panic is joined and turned into an error, and the
/// message still reaches stderr.
pub fn looks_like_panic(stderr: &str) -> bool {
    stderr.contains("panicked at")
        || stderr.contains("internal error")
        || stderr.contains("has overflowed its stack")
        || stderr.contains("RUST_BACKTRACE")
}

/// A short, stable signature for deduplicating findings: the first panic line
/// or signal number.
pub fn signature(result: &RunResult) -> String {
    match &result.outcome {
        Outcome::Signal(s) => format!("signal-{s}"),
        Outcome::Timeout => "timeout".to_string(),
        Outcome::MemoryLimit => "memory".to_string(),
        Outcome::OutputBomb => "output".to_string(),
        Outcome::PanicText(_) | Outcome::Clean(_) => {
            for line in result.stderr_tail.lines() {
                if line.contains("panicked at") || line.contains("internal error") {
                    // Strip the volatile memory address if any.
                    let line = line.split("0x").next().unwrap_or(line);
                    return format!("panic: {}", &line[..line.len().min(160)]);
                }
            }
            format!("exit-{:?}", result.outcome)
        }
    }
}

/// Writes the case and its metadata into `dir` for later minimisation.
pub fn save_case(dir: &Path, source: &str, result: &RunResult, note: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("create findings dir");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file = dir.join(format!("case-{stamp}.php"));
    fs::write(&file, source).expect("write case");
    let meta = format!(
        "command: {}\noutcome: {:?}\nelapsed_ms: {}\npeak_rss_kib: {}\nstdout_bytes: {}\nnote: {note}\nstderr_tail:\n{}\n",
        result.command,
        result.outcome,
        result.elapsed.as_millis(),
        result.peak_rss_kib,
        result.stdout_bytes,
        result.stderr_tail
    );
    fs::write(file.with_extension("php.meta.txt"), meta).expect("write meta");
    file
}
