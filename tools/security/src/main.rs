//! `phera-fuzz`: defensive security fuzzing harness for the Phera engine.
//!
//! Campaigns:
//!   seeds   extract PHP seeds from .phpt/.php files into a corpus directory
//!   mutate  mutation fuzzer over the corpus (mode run; `dump` = --dump only)
//!   gen     structure-aware generator (nesting, chains, recursion, decoders)
//!   funcs   hostile arguments for native functions
//!
//! Every campaign writes progress lines to <out>/progress.log and saves every
//! crashing/hanging input to <out>/findings/ with a .meta.txt sidecar.
//!
//! Run under limits, e.g.:
//!   nice -n 15 ./phera-fuzz mutate --corpus seeds --out run1 \
//!     --phera ../../target/release/phera --seconds 7200 --jobs 2

mod funcs;
mod gen;
mod mutate;
mod rng;
mod runner;
mod seeds;

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rng::Rng;
use runner::{Limits, Outcome};

struct Stats {
    executed: AtomicU64,
    clean: AtomicU64,
    findings: AtomicU64,
    signatures: Mutex<HashSet<String>>,
}

impl Stats {
    fn new() -> Self {
        Stats {
            executed: AtomicU64::new(0),
            clean: AtomicU64::new(0),
            findings: AtomicU64::new(0),
            signatures: Mutex::new(HashSet::new()),
        }
    }
}

fn progress(out: &Path, msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(out.join("progress.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

/// Executes one source string and records findings. Returns the outcome.
fn execute(
    phera: &str,
    mode: &str,
    workdir: &Path,
    limits: &Limits,
    stats: &Stats,
    out: &Path,
    source: &str,
    note: &str,
) -> Outcome {
    let case_path = workdir.join("case.php");
    fs::write(&case_path, source).expect("write case");
    let result = runner::run_case(phera, mode, &case_path, workdir, limits);
    stats.executed.fetch_add(1, Ordering::Relaxed);
    if result.outcome.is_finding() {
        stats.findings.fetch_add(1, Ordering::Relaxed);
        let sig = runner::signature(&result);
        let new = stats.signatures.lock().unwrap().insert(sig.clone());
        let path = runner::save_case(&out.join("findings"), source, &result, note);
        progress(
            out,
            &format!(
                "FINDING {} sig={} new={} file={} note={} stderr_first_line={}",
                result.outcome.name(),
                &sig[..sig.len().min(120)],
                new,
                path.display(),
                note,
                result.stderr_tail.lines().next().unwrap_or("")
            ),
        );
    } else {
        stats.clean.fetch_add(1, Ordering::Relaxed);
    }
    result.outcome
}

struct Common {
    phera: String,
    out: PathBuf,
    seconds: u64,
    jobs: usize,
    mode: String,
    timeout_ms: u64,
    seed: u64,
    corpus: PathBuf,
}

fn parse_common(args: &[String]) -> Common {
    let mut c = Common {
        phera: String::new(),
        out: PathBuf::from("fuzz-out"),
        seconds: 600,
        jobs: 2,
        mode: "run".to_string(),
        timeout_ms: 5000,
        seed: 0x5eed,
        corpus: PathBuf::from("corpus"),
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--phera" => {
                i += 1;
                c.phera = args[i].clone();
            }
            "--out" => {
                i += 1;
                c.out = PathBuf::from(&args[i]);
            }
            "--seconds" => {
                i += 1;
                c.seconds = args[i].parse().unwrap();
            }
            "--jobs" => {
                i += 1;
                c.jobs = args[i].parse().unwrap();
            }
            "--mode" => {
                i += 1;
                c.mode = args[i].clone();
            }
            "--timeout-ms" => {
                i += 1;
                c.timeout_ms = args[i].parse().unwrap();
            }
            "--seed" => {
                i += 1;
                c.seed = args[i].parse().unwrap();
            }
            "--corpus" => {
                i += 1;
                c.corpus = PathBuf::from(&args[i]);
            }
            other => panic!("unknown option: {other}"),
        }
        i += 1;
    }
    assert!(!c.phera.is_empty(), "--phera is required");
    c
}

fn limits(c: &Common) -> Limits {
    Limits {
        timeout: Duration::from_millis(c.timeout_ms),
        ..Limits::default()
    }
}

/// Runs `body(job)` on `jobs` threads until `deadline`, logging progress every
/// 30 s, then prints the totals.
fn run_parallel<F>(c: &Common, body: F)
where
    F: Fn(usize, Arc<Stats>, &Common, Instant) + Send + Sync + 'static,
{
    fs::create_dir_all(&c.out).expect("create out dir");
    let stats = Arc::new(Stats::new());
    let deadline = Instant::now() + Duration::from_secs(c.seconds);
    let monitor_stats = stats.clone();
    let monitor_out = c.out.clone();
    let monitor_deadline = deadline;
    let monitor = std::thread::spawn(move || {
        while Instant::now() < monitor_deadline {
            std::thread::sleep(Duration::from_secs(30));
            progress(
                &monitor_out,
                &format!(
                    "PROGRESS executed={} clean={} findings={}",
                    monitor_stats.executed.load(Ordering::Relaxed),
                    monitor_stats.clean.load(Ordering::Relaxed),
                    monitor_stats.findings.load(Ordering::Relaxed)
                ),
            );
        }
    });
    let body = Arc::new(body);
    let mut handles = Vec::new();
    for job in 0..c.jobs.max(1) {
        let body = body.clone();
        let stats = stats.clone();
        let out = c.out.clone();
        let phera = c.phera.clone();
        let mode = c.mode.clone();
        let secs = c.seconds;
        let seed = c.seed;
        let timeout_ms = c.timeout_ms;
        handles.push(std::thread::spawn(move || {
            let common = Common {
                phera,
                out,
                seconds: secs,
                jobs: 1,
                mode,
                timeout_ms,
                seed,
                corpus: PathBuf::from("corpus"),
            };
            body(job, stats, &common, deadline);
        }));
    }
    for h in handles {
        h.join().expect("worker panicked");
    }
    monitor.join().expect("monitor panicked");
    progress(
        &c.out,
        &format!(
            "DONE executed={} clean={} findings={}",
            stats.executed.load(Ordering::Relaxed),
            stats.clean.load(Ordering::Relaxed),
            stats.findings.load(Ordering::Relaxed)
        ),
    );
    println!(
        "campaign done: executed={} clean={} findings={} -> {}",
        stats.executed.load(Ordering::Relaxed),
        stats.clean.load(Ordering::Relaxed),
        stats.findings.load(Ordering::Relaxed),
        c.out.join("progress.log").display()
    );
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = argv.first() else {
        eprintln!("usage: phera-fuzz <seeds|mutate|dump|gen|funcs> [options]");
        std::process::exit(2);
    };
    let rest = &argv[1..];
    match command.as_str() {
        "seeds" => {
            let mut roots = Vec::new();
            let mut corpus = PathBuf::from("corpus");
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--root" => {
                        i += 1;
                        roots.push(PathBuf::from(&rest[i]));
                    }
                    "--corpus" => {
                        i += 1;
                        corpus = PathBuf::from(&rest[i]);
                    }
                    other => panic!("unknown option: {other}"),
                }
                i += 1;
            }
            let n = seeds::collect(&roots, &corpus).expect("collect seeds");
            println!("wrote {n} seeds to {}", corpus.display());
        }
        "mutate" | "dump" => {
            let mut c = parse_common(rest);
            if command == "dump" {
                c.mode = "dump".to_string();
                c.timeout_ms = c.timeout_ms.max(10000);
            }
            let corpus = c.corpus.clone();
            let seeds = Arc::new(seeds::load(&corpus));
            assert!(!seeds.is_empty(), "no seeds in {}", corpus.display());
            println!(
                "mutation campaign: {} seeds, mode={}, jobs={}, {}s",
                seeds.len(),
                c.mode,
                c.jobs,
                c.seconds
            );
            let seeds2 = seeds.clone();
            run_parallel(&c, move |job, stats, c, deadline| {
                let mut rng = Rng::new(c.seed ^ (job as u64).wrapping_mul(0x9E3779B97F4A7C15));
                let workdir = c.out.join(format!("work-{job}"));
                fs::create_dir_all(&workdir).expect("workdir");
                let limits = limits(c);
                while Instant::now() < deadline {
                    let s = rng.below(seeds2.len());
                    let other = rng.below(seeds2.len());
                    let count = 1 + rng.below(6);
                    let seed_a = seeds2[s].clone();
                    let seed_b = seeds2[other].clone();
                    let (mutated, log) = mutate::mutate(&seed_a, &seed_b, &mut rng, count, 128 * 1024);
                    let source = String::from_utf8_lossy(&mutated).into_owned();
                    let note = format!("seed={s} other={other} ops: {log}");
                    execute(&c.phera, &c.mode, &workdir, &limits, &stats, &c.out, &source, &note);
                }
            });
        }
        "gen" => {
            let c = parse_common(rest);
            run_parallel(&c, move |job, stats, c, deadline| {
                let mut rng = Rng::new(c.seed ^ (job as u64 + 1).wrapping_mul(0x2545F4914F6CDD1D));
                let workdir = c.out.join(format!("work-{job}"));
                fs::create_dir_all(&workdir).expect("workdir");
                let limits = limits(c);
                let mut round = 0u64;
                while Instant::now() < deadline {
                    round += 1;
                    for scale in gen::SCALES {
                        for case in gen::cases(*scale, &mut rng) {
                            if Instant::now() >= deadline {
                                return;
                            }
                            let note = format!("gen {} round {round}", case.name);
                            execute(&c.phera, &c.mode, &workdir, &limits, &stats, &c.out, &case.source, &note);
                        }
                    }
                }
            });
        }
        "funcs" => {
            let c = parse_common(rest);
            run_parallel(&c, move |job, stats, c, deadline| {
                let mut rng = Rng::new(c.seed ^ (job as u64 + 7).wrapping_mul(0x9E3779B97F4A7C15));
                let all = funcs::cases(&mut rng);
                let workdir = c.out.join(format!("work-{job}"));
                fs::create_dir_all(&workdir).expect("workdir");
                let limits = limits(c);
                let mut i = job;
                while Instant::now() < deadline {
                    if i >= all.len() {
                        i = 0;
                    }
                    let (call, source) = &all[i];
                    i += 1;
                    let note = format!("funcs {call}");
                    execute(&c.phera, &c.mode, &workdir, &limits, &stats, &c.out, source, &note);
                }
            });
        }
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    }
}
