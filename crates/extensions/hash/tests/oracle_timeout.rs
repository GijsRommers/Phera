//! An invalid xxHash secret must throw even with data longer than the streaming buffer.
//! PHP 8.5.12 currently loops in XXH3_update for these cases; keep that failure visible.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, Extension, PhpArray, Registry, Value};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn invalid_xxhash_secret_long_input() {
    let php = oracle::find_php().expect("PHP required");
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut failures = Vec::new();
    for algo in ["xxh3", "xxh128"] {
        for size in [0, 1, 135] {
            let mut context = TestContext::default();
            let mut options = PhpArray::new();
            options.insert(ArrayKey::str("secret"), Value::from("s".repeat(size)));
            let result = call(
                &registry,
                &mut context,
                "hash",
                vec![
                    Value::from(algo),
                    Value::from("x".repeat(1024)),
                    Value::Bool(false),
                    Value::from(options),
                ],
            );
            let expected =
                oracle::encode_case("case", &result, &context.output, &context.diagnostics);
            let script = format!(
                "{} \
                run('case',fn()=>hash('{algo}',str_repeat('x',1024),false,\
                ['secret'=>str_repeat('s',{size})]));",
                oracle::PHP_PREAMBLE
            );
            let mut child = Command::new(&php)
                .args(["-n", "-r", &script])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            loop {
                if child.try_wait().unwrap().is_some() {
                    let output = child.wait_with_output().unwrap();
                    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if actual != expected {
                        failures.push(format!(
                            "{algo} secret={size}: PHP {actual}; Phera {expected}"
                        ));
                    }
                    break;
                }
                if start.elapsed() > Duration::from_secs(2) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    failures.push(format!(
                        "{algo} secret={size}: PHP oracle timed out in \
                XXH3_update; Phera {expected}"
                    ));
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
