//! PHP 8.5's CLI checks session signatures, lifecycle and object callbacks.

use std::path::Path;
use std::process::Command;

use phera_extension_api::testing::{
    oracle::find_php,
    scripts::{Comparison, Runtime},
};

const CALLBACKS: &str = include_str!("session_oracle/callbacks.php");
const CALLS: &str = include_str!("session_oracle/functions.php");
const REDIS: &str = include_str!("session_oracle/redis.php");

fn oracle(name: &str, source: &str) -> (Comparison, std::path::PathBuf) {
    let comparison = Comparison::for_binary(name, Path::new(env!("CARGO_BIN_EXE_phera")));
    let script = comparison.artifacts.join("script.php");
    std::fs::write(&script, source).expect("write the oracle script");
    (comparison, script)
}

#[test]
fn object_handler_failures_match_php() {
    let (comparison, script) = oracle("session-callbacks", CALLBACKS);
    let matched = comparison
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error} in {}", comparison.artifacts.display()));
    assert_eq!(String::from_utf8_lossy(&matched.stdout).lines().count(), 63);
}

#[test]
fn session_calls_match_php() {
    let (comparison, script) = oracle("session-functions", CALLS);
    let matched = comparison
        .compare(&script, |runtime, command| {
            command.arg(comparison.scratch_directory(&script, runtime));
        })
        .unwrap_or_else(|error| panic!("{error} in {}", comparison.artifacts.display()));
    assert_eq!(
        String::from_utf8_lossy(&matched.stdout).lines().count(),
        2706
    );
}

#[test]
fn redis_user_handler_can_suspend() {
    let php = find_php().expect("PHP 8.5 CLI oracle");
    let available = Command::new(php)
        .args(["-n", "-r", "exit(extension_loaded('redis') ? 0 : 1);"])
        .status()
        .is_ok_and(|status| status.success());
    if !available || std::net::TcpStream::connect("127.0.0.1:16399").is_err() {
        eprintln!("SKIP: phpredis and the test Redis server are required");
        return;
    }
    let (comparison, script) = oracle("session-redis", REDIS);
    comparison
        .compare(&script, |runtime, command| {
            let name = match runtime {
                Runtime::Php => "php",
                Runtime::Phera => "phera",
            };
            command.arg(format!("phera:session:{}:{name}:", std::process::id()));
        })
        .unwrap_or_else(|error| panic!("{error} in {}", comparison.artifacts.display()));
}
