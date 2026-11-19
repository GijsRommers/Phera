//! A secret shorter than 136 bytes is an error for `xxh3` and `xxh128`:
//!
//! ```php
//! hash('xxh3', 'abc', false, ['secret' => 'short']);
//! // Error: xxh3: Secret length must be >= 136 bytes, 5 bytes passed
//! ```
//!
//! PHP may stop returning from this call, depending on the release and the
//! input length. Keep these cases out of a shared oracle script so that an
//! unanswered case cannot prevent the remaining comparisons from running.
//! Every Phera call must throw. Every completed PHP call must agree with Phera.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, NativeError, PhpArray, Value};

use crate::support::registry;

/// Far longer than PHP needs for a call that returns.
const TIME_LIMIT: Duration = Duration::from_secs(2);

/// `hash($algo, str_repeat('x', $length), false, ['secret' => ...])`
fn expression(algo: &str, length: usize, secret: usize) -> String {
    format!(
        "hash('{algo}', str_repeat('x', {length}), false, ['secret' => str_repeat('s', {secret})])"
    )
}

/// Starts PHP on one case; it prints the case line, if it ever finishes.
fn spawn_php(expression: &str) -> Child {
    let php = oracle::find_php().expect("the selected PHP oracle is required");
    let script = format!(
        "{} run('case', fn() => {expression});",
        oracle::PHP_PREAMBLE
    );
    Command::new(php)
        .args(["-n", "-r", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start php")
}

/// What `child` printed, or `None` if it did not finish by `deadline`, in
/// which case it is killed.
fn output_until(mut child: Child, deadline: Instant) -> Option<String> {
    loop {
        if child.try_wait().expect("poll php").is_some() {
            let output = child.wait_with_output().expect("read php output");
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop php");
            child.wait().expect("reap php");
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn short_secrets_with_bounded_php_oracle() {
    let registry = registry();
    let mut cases = Vec::new();
    for algo in ["xxh3", "xxh128"] {
        for secret in [0, 1, 135] {
            let mut options = PhpArray::new();
            options.insert(ArrayKey::str("secret"), Value::from("s".repeat(secret)));
            for length in [0, 1, 17, 240, 241, 1024] {
                let arguments = vec![
                    Value::from(algo),
                    Value::from("x".repeat(length)),
                    Value::Bool(false),
                    Value::from(options.clone()),
                ];
                let mut context = TestContext::default();
                let result = call(&registry, &mut context, "hash", arguments);
                let message =
                    format!("{algo}: Secret length must be >= 136 bytes, {secret} bytes passed");
                assert!(
                    matches!(&result, Err(NativeError::Throw { class: "Error", message: actual, .. })
                        if *actual == message),
                    "{algo} with a {secret}-byte secret must throw: {result:?}"
                );
                let line =
                    oracle::encode_case("case", &result, &context.output, &context.diagnostics);
                let expression = expression(algo, length, secret);
                cases.push((expression, line));
            }
        }
    }

    let pending: Vec<_> = cases
        .into_iter()
        .map(|(expression, line)| (spawn_php(&expression), expression, line))
        .collect();
    // All case scripts already started, so they can share the same deadline.
    let deadline = Instant::now() + TIME_LIMIT;
    let results: Vec<_> = pending
        .into_iter()
        .map(|(child, expression, line)| (output_until(child, deadline), expression, line))
        .collect();
    let tested = results.len();
    let mut unanswered = 0;
    for (php, expression, phera) in results {
        match php {
            Some(php) => assert_eq!(php, phera, "{expression}"),
            None => unanswered += 1,
        }
    }
    println!(
        "short xxHash secrets: PHP {} did not answer {unanswered} of {tested} calls",
        oracle::test_level().name()
    );
}
