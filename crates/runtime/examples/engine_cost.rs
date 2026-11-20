//! What the engine costs without the snapshot hiding it: boots, requests
//! run from the top, and forked requests, through the runtime's own API.
//!
//! ```sh
//! cargo build --release -p phera-runtime --example engine_cost
//! target/release/examples/engine_cost <document root> <route> boot 2   # two workers' boots
//! target/release/examples/engine_cost <document root> <route> top 100  # 100 requests from the top
//! target/release/examples/engine_cost <document root> <route> fork 100 # boot, then 100 forks
//! ```
//!
//! `phera serve` always forks; this program exists so that the cost of
//! loading, linking and running an application from the top stays a number
//! the project tracks. It prints wall time per boot and the status of the
//! last response; instructions come from running it under `/usr/bin/time
//! -l` with two counts and taking the difference. Every mode first runs
//! three requests (or none, for `boot`) that are not counted.

use std::path::Path;
use std::time::Instant;

use phera_runtime::{BootReport, HttpRequest, Runtime, WebWorker, adapters};

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [root, route, mode, count] = arguments.as_slice() else {
        eprintln!("usage: engine_cost <document root> <route> boot|top|fork <count>");
        return std::process::ExitCode::FAILURE;
    };
    let root = match Path::new(root).canonicalize() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{root}: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let Ok(count) = count.parse::<usize>() else {
        eprintln!("the count must be a number");
        return std::process::ExitCode::FAILURE;
    };
    let script = root.join("index.php");
    let runtime = runtime_for(&root, &script);
    let threads = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(threads) = threads else {
        eprintln!("cannot start the async runtime");
        return std::process::ExitCode::FAILURE;
    };
    let local = tokio::task::LocalSet::new();
    let result = local.block_on(&threads, async {
        match mode.as_str() {
            "boot" => {
                for index in 0..count {
                    let worker = runtime.web_worker(index);
                    let started = Instant::now();
                    let report = worker.boot(&script, &root).await;
                    eprintln!(
                        "boot {index}: {} ms, {}",
                        started.elapsed().as_millis(),
                        describe(&report)
                    );
                }
                Ok(())
            }
            "top" => requests(&runtime.web_worker(0), &script, &root, route, count).await,
            "fork" => {
                let worker = runtime.web_worker(0);
                match worker.boot(&script, &root).await {
                    BootReport::Snapshot { .. } => {}
                    report => {
                        return Err(format!("the boot did not snapshot: {}", describe(&report)));
                    }
                }
                requests(&worker, &script, &root, route, count).await
            }
            other => Err(format!("unknown mode {other}")),
        }
    });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The runtime as `phera serve` sets it up for this document root.
fn runtime_for(root: &Path, script: &Path) -> Runtime {
    let mut environment = phera_dotenv::Environment::process();
    if adapters::uses_symfony_runtime(root) && environment.get(b"APP_RUNTIME").is_none() {
        environment.insert(
            b"APP_RUNTIME".to_vec(),
            adapters::SYMFONY_RUNTIME_CLASS.as_bytes().to_vec(),
        );
    }
    let runtime = Runtime::with_environment(environment);
    if adapters::is_laravel(root) {
        runtime.with_entry_script(script.to_path_buf(), adapters::LARAVEL_ENTRY)
    } else {
        runtime
    }
}

async fn requests(
    worker: &WebWorker,
    script: &Path,
    root: &Path,
    route: &str,
    count: usize,
) -> Result<(), String> {
    let mut status = 0;
    for _ in 0..count + 3 {
        let response = worker.handle_request(script, root, request(route)).await;
        status = response.status;
    }
    eprintln!("last status {status}");
    Ok(())
}

fn request(target: &str) -> HttpRequest {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    HttpRequest {
        method: "GET".to_string(),
        target: target.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        protocol: "HTTP/1.1".to_string(),
        headers: vec![
            ("Host".to_string(), b"127.0.0.1".to_vec()),
            ("Accept".to_string(), b"application/json".to_vec()),
        ],
        body: Vec::new(),
        remote_addr: "127.0.0.1:50000".parse().expect("a socket address"),
        local_addr: "127.0.0.1:8080".parse().expect("a socket address"),
        https: false,
    }
}

fn describe(report: &BootReport) -> String {
    match report {
        BootReport::Snapshot { files, .. } => format!("snapshot, {files} files"),
        BootReport::Classic => "no Phera\\snapshot() call".into(),
        BootReport::Failed(message) => format!("failed: {message}"),
    }
}
