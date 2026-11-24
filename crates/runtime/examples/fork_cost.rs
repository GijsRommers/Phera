//! What one fork costs, on a synthetic application the program writes
//! itself: bytes and allocations per fork, time per fork, and the same per
//! request.
//!
//! ```sh
//! cargo build --release -p phera-runtime --example fork_cost
//! target/release/examples/fork_cost fork 10000                  # fork and drop, no request
//! target/release/examples/fork_cost request 2000 'touch=4'      # a request that runs 4 files
//! target/release/examples/fork_cost request 2000 'touch=4&late=70'
//! # a request that also loads and declares 70 classes the boot never loaded
//! target/release/examples/fork_cost --settle request 2000 'touch=4&late=70'
//! # the same after the worker booted again with those classes declared
//! # options before the mode: --files 40 --classes 8 --dir <where to write the site>
//! ```
//!
//! The application is a few dozen files of classes, methods and functions
//! (generated below), a boot that runs every method once so that the
//! snapshot holds filled caches, and `late/` files the boot does not load.
//! This program counts no allocations: a counting allocator is `unsafe`
//! code, which no crate of the workspace may hold. The tool the project
//! keeps outside the repository (a package of its own with
//! `#[path = ".../fork_cost.rs"] mod harness;` and a counting
//! `GlobalAlloc`) calls [`run`] with a function that returns the totals;
//! here they are always zero. For machine instructions run it under
//! `perf stat -e instructions:u` with two counts and take the difference.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use phera_runtime::{BootReport, HttpRequest, Runtime};

/// Returns the allocations made so far and the bytes asked for, by the
/// whole process.
pub type Counts = fn() -> (u64, u64);

/// The shape of the synthetic application.
struct Shape {
    files: usize,
    classes: usize,
    late: usize,
}

fn main() -> std::process::ExitCode {
    run(|| (0, 0))
}

/// The whole program, with the allocation counts of whoever runs it.
pub fn run(counts: Counts) -> std::process::ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    // `--settle`: let the worker boot again with the classes its requests
    // autoloaded declared in the snapshot, before anything is measured.
    let settle = arguments.iter().any(|argument| argument == "--settle");
    arguments.retain(|argument| argument != "--settle");
    let mut shape = Shape {
        files: 40,
        classes: 8,
        late: 70,
    };
    let mut directory =
        std::env::temp_dir().join(format!("phera-fork-cost-{}", std::process::id()));
    while arguments
        .first()
        .is_some_and(|argument| argument.starts_with("--"))
    {
        let option = arguments.remove(0);
        if arguments.is_empty() {
            eprintln!("{option} needs a value");
            return std::process::ExitCode::FAILURE;
        }
        let value = arguments.remove(0);
        match (option.as_str(), value.parse::<usize>()) {
            ("--files", Ok(number)) => shape.files = number,
            ("--classes", Ok(number)) => shape.classes = number,
            ("--late", Ok(number)) => shape.late = number,
            ("--dir", _) => directory = PathBuf::from(value),
            _ => {
                eprintln!("unknown option {option}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    let [mode, count, rest @ ..] = arguments.as_slice() else {
        eprintln!(
            "usage: fork_cost [--files N] [--classes N] [--late N] [--dir D] fork|request <count> [query]"
        );
        return std::process::ExitCode::FAILURE;
    };
    let Ok(count) = count.parse::<usize>() else {
        eprintln!("the count must be a number");
        return std::process::ExitCode::FAILURE;
    };
    let query = rest.first().map_or("", String::as_str);
    if let Err(error) = write_site(&directory, &shape) {
        eprintln!("cannot write the site: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let root = directory.canonicalize().expect("the site directory exists");
    let script = root.join("index.php");
    let runtime = Runtime::new();
    let threads = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the async runtime starts");
    let local = tokio::task::LocalSet::new();
    let result = local.block_on(&threads, async {
        let worker = match settle {
            true => runtime.web_worker(0).with_background_boots(),
            false => runtime.web_worker(0),
        };
        let started = Instant::now();
        match worker.boot(&script, &root).await {
            BootReport::Snapshot { files, .. } => {
                eprintln!(
                    "boot: {files} files in {} ms",
                    started.elapsed().as_millis()
                );
            }
            BootReport::Classic => return Err("the boot did not snapshot".to_string()),
            BootReport::Failed(message) => return Err(format!("the boot failed: {message}")),
        }
        match mode.as_str() {
            "fork" => {
                for _ in 0..3 {
                    if let Some(Ok(mut fork)) = worker.fork_snapshot(&script) {
                        fork.recycle();
                    }
                }
                let (allocations, bytes, started) = counters(counts);
                for _ in 0..count {
                    let mut fork = worker
                        .fork_snapshot(&script)
                        .expect("a snapshot")
                        .map_err(|error| format!("fork: {error}"))?;
                    fork.recycle();
                    drop(fork);
                }
                report("fork", count, (allocations, bytes), started, counts);
            }
            "request" => {
                let mut body = Vec::new();
                for _ in 0..3 {
                    let request = request(query);
                    body = worker.handle_request(&script, &root, request).await.body;
                }
                if settle {
                    // Requests until the worker starts its warm-up boot, then
                    // time for the boot to finish and a request on the new snapshot.
                    for _ in 0..1000 {
                        if worker.warmup_boots(&script) > 0 {
                            break;
                        }
                        body = worker
                            .handle_request(&script, &root, request(query))
                            .await
                            .body;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                    for _ in 0..3 {
                        body = worker
                            .handle_request(&script, &root, request(query))
                            .await
                            .body;
                    }
                    eprintln!(
                        "settled: {} warm-up boot(s), {} classes declared in the snapshot",
                        worker.warmup_boots(&script),
                        worker.warmup_classes(&script).len()
                    );
                }
                let (allocations, bytes, started) = counters(counts);
                for _ in 0..count {
                    let request = request(query);
                    body = worker.handle_request(&script, &root, request).await.body;
                }
                report("request", count, (allocations, bytes), started, counts);
                eprintln!("body: {}", String::from_utf8_lossy(&body).trim());
            }
            other => return Err(format!("unknown mode {other}")),
        }
        Ok(())
    });
    if directory.starts_with(std::env::temp_dir()) {
        let _ = std::fs::remove_dir_all(&directory);
    }
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn counters(counts: Counts) -> (u64, u64, Instant) {
    let (allocations, bytes) = counts();
    (allocations, bytes, Instant::now())
}

fn report(what: &str, count: usize, before: (u64, u64), started: Instant, counts: Counts) {
    let elapsed = started.elapsed();
    let (allocations, bytes) = counts();
    let count = count.max(1) as u64;
    eprintln!(
        "{what}: {} allocations, {} bytes, {} ns per {what}",
        (allocations - before.0) / count,
        (bytes - before.1) / count,
        elapsed.as_nanos() / u128::from(count),
    );
}

fn request(target: &str) -> HttpRequest {
    let target = format!("/index.php?{target}");
    HttpRequest {
        method: "GET".to_string(),
        path: "/index.php".to_string(),
        query: target
            .split_once('?')
            .map_or("", |(_, query)| query)
            .to_string(),
        target,
        protocol: "HTTP/1.1".to_string(),
        headers: vec![("Host".to_string(), b"127.0.0.1".to_vec())],
        body: Vec::new(),
        remote_addr: "127.0.0.1:50000".parse().expect("a socket address"),
        local_addr: "127.0.0.1:8080".parse().expect("a socket address"),
        https: false,
    }
}

/// The classes of file `file`: each uses properties, typed parameters,
/// closures, static and named calls, so that running them fills every kind
/// of cache.
fn class_file(file: usize, shape: &Shape) -> String {
    let mut source = String::from("<?php\n");
    writeln!(source, "trait T{file} {{ private int $n = 0; public function bump(): int {{ return ++$this->n; }} }}").unwrap();
    writeln!(
        source,
        "function helper{file}(int $v): int {{ return $v * 2 + {file}; }}"
    )
    .unwrap();
    for class in 0..shape.classes {
        let previous = if class == 0 { class } else { class - 1 };
        write!(
            source,
            "final class C{file}_{class} {{\n    use T{file};\n    private array $items = [];\n    \
             public function __construct(private int $a = 1, protected string $b = 'x', public ?C{file}_{previous} $link = null) {{}}\n    \
             public function m0(int $x): int {{ $this->items[] = $x; return $this->a + $x + $this->bump(); }}\n    \
             public function m1(int $x): string {{ return $this->b . strlen((string)$x) . helper{file}(1); }}\n    \
             public function m2(): int {{ $f = fn(int $v) => $v + $this->a; return $f(2) + count($this->items); }}\n    \
             public function m3(array $xs): int {{ $t = 0; foreach ($xs as $x) {{ $t += $x * $this->a; }} return $t; }}\n    \
             public function m4(self $o): int {{ return $o->m0(1) + $this->m0(2); }}\n    \
             public function m5(int $x = 3): ?string {{ return $this->link?->m1($x) ?? $this->b; }}\n    \
             public function m6(): int {{ return $this->items[0] ?? 0; }}\n    \
             public function m7(int $x): int {{ return helper{file}($x) + $this->m6(); }}\n    \
             public function m8(string $s): string {{ return strtoupper($s) . $this->b; }}\n    \
             public function m9(int $x): int {{ return max($x, $this->a) + abs($x); }}\n}}\n"
        )
        .unwrap();
    }
    writeln!(source, "function run{file}(): int {{\n    $t = 0;").unwrap();
    for class in 0..shape.classes {
        writeln!(
            source,
            "    $o = new C{file}_{class}(2, 'y'); $t += $o->m0(1); $t += strlen($o->m1(2)); $t += $o->m2(); \
             $t += $o->m3([1, 2, 3]); $t += $o->m4($o); $t += strlen((string)$o->m5()); $t += $o->m6(); \
             $t += $o->m7(4); $t += strlen($o->m8('s')); $t += $o->m9(5);"
        )
        .unwrap();
    }
    writeln!(source, "    return $t;\n}}").unwrap();
    source
}

fn write_site(directory: &Path, shape: &Shape) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory.join("app"))?;
    std::fs::create_dir_all(directory.join("late"))?;
    let mut index = String::from("<?php\n");
    for file in 0..shape.files {
        std::fs::write(
            directory.join(format!("app/f{file}.php")),
            class_file(file, shape),
        )?;
        writeln!(index, "require_once __DIR__ . '/app/f{file}.php';").unwrap();
    }
    for late in 0..shape.late {
        let source = format!(
            "<?php\nfunction late_helper{late}(int $v): int {{ return $v + {late}; }}\n\
             class Late{late} {{\n    public function __construct(private int $v = 1) {{}}\n    \
             public function go(int $x): int {{ return $this->v + $x + late_helper{late}($x); }}\n    \
             public function more(array $xs): int {{ return array_sum($xs) + $this->v; }}\n}}\n"
        );
        std::fs::write(directory.join(format!("late/Late{late}.php")), source)?;
    }
    writeln!(
        index,
        "spl_autoload_register(function (string $c) {{ if (str_starts_with($c, 'Late')) require __DIR__ . '/late/' . $c . '.php'; }});\n\
         $objs = [];\n\
         for ($i = 0; $i < {files}; $i++) {{ $f = 'run' . $i; $f(); }}",
        files = shape.files
    )
    .unwrap();
    for file in 0..shape.files {
        for class in 0..shape.classes.min(5) {
            writeln!(index, "$objs[] = new C{file}_{class}(1, 'z');").unwrap();
        }
    }
    writeln!(
        index,
        "Phera\\snapshot();\n\
         $touch = (int)($_GET['touch'] ?? 0);\n\
         $late = (int)($_GET['late'] ?? 0);\n\
         $total = count($objs);\n\
         for ($i = 0; $i < $touch; $i++) {{ $f = 'run' . $i; $total += $f(); }}\n\
         for ($i = 0; $i < $late; $i++) {{ $c = 'Late' . $i; $o = new $c; $total += $o->go(2) + $o->more([1, 2]); }}\n\
         echo $total, \"\\n\";"
    )
    .unwrap();
    std::fs::write(directory.join("index.php"), index)
}
