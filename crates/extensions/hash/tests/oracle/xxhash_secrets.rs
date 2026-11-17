//! A secret shorter than 136 bytes is an error for `xxh3` and `xxh128`:
//!
//! ```php
//! hash('xxh3', 'abc', false, ['secret' => 'short']);
//! // Error: xxh3: Secret length must be >= 136 bytes, 5 bytes passed
//! ```
//!
//! PHP 8.5.12 throws that error only when the data is short. With 1 KiB of
//! data it never returns: the error is raised while the state is set up,
//! but `hash()` goes on to feed the data through the state that was not
//! set up, and `XXH3_update` loops (still running after 20 seconds on an
//! Apple M4; it answers at once for up to 241 bytes). Phera throws the
//! error whatever the length of the data.
//!
//! So for 1 KiB of data PHP gets a time limit. If it answers within it,
//! the answer must be Phera's; PHP 8.5.12 does not answer, which the test
//! reports. Either way Phera's answer must also be what PHP answers for the
//! same secret and 240 bytes of data.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, NativeError, PhpArray, Value};

use crate::support::{Comparison, registry};

/// Far longer than PHP needs for a call that returns.
const TIME_LIMIT: Duration = Duration::from_secs(2);

/// Long enough for `XXH3_update` to take its streaming path in PHP.
const LONG: usize = 1024;

/// Short enough for PHP to return.
const SHORT: usize = 240;

/// `hash($algo, str_repeat('x', $length), false, ['secret' => ...])`
fn expression(algo: &str, length: usize, secret: usize) -> String {
    format!(
        "hash('{algo}', str_repeat('x', {length}), false, ['secret' => str_repeat('s', {secret})])"
    )
}

/// Starts PHP on one case; it prints the case line, if it ever finishes.
fn spawn_php(expression: &str) -> Child {
    let php = oracle::find_php().expect("PHP 8.5 required");
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
    while Instant::now() < deadline {
        if child.try_wait().expect("poll php").is_some() {
            let output = child.wait_with_output().expect("read php output");
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().expect("stop php");
    child.wait().expect("reap php");
    None
}

#[test]
fn short_secrets_with_long_data() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut short_data = Comparison::new();
    let mut long_data = Vec::new();
    for algo in ["xxh3", "xxh128"] {
        for secret in [0, 1, 135] {
            let mut options = PhpArray::new();
            options.insert(ArrayKey::str("secret"), Value::from("s".repeat(secret)));
            let data = Value::from("x".repeat(LONG));
            let arguments = vec![
                Value::from(algo),
                data,
                Value::Bool(false),
                Value::from(options),
            ];
            let result = call(&registry, &mut context, "hash", arguments);
            assert!(
                matches!(&result, Err(NativeError::Throw { class: "Error", message, .. })
                    if message.contains("Secret length must be >= 136 bytes")),
                "{algo} with a {secret}-byte secret must throw"
            );
            let line = oracle::encode_case("case", &result, &context.output, &context.diagnostics);
            let long = expression(algo, LONG, secret);
            long_data.push((long.clone(), spawn_php(&long), line));
            short_data.record_expression(&expression(algo, SHORT, secret), &result, &mut context);
        }
    }

    // The PHP processes run side by side, so the whole test waits about one
    // time limit.
    let deadline = Instant::now() + TIME_LIMIT;
    let mut unanswered = 0;
    for (expression, child, phera) in long_data {
        match output_until(child, deadline) {
            Some(php) => assert_eq!(php, phera, "{expression}"),
            None => unanswered += 1,
        }
    }
    println!("short xxHash secrets: PHP did not answer {unanswered} of 6 calls with {LONG} bytes");
    short_data.compare(
        "hash-xxhash-short-secrets",
        "short xxHash secrets, compared with PHP's answer for 240 bytes",
    );
}
