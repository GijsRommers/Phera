//! Run one generated native call table in PHP and in the registered Rust extension.

use std::fs;
use std::path::Path;
use std::process::Command;

use phera_ext_streams::{Streams, set_request};
use phera_extension_api::{
    NativeFn, Registry, Value, prepare_arguments,
    testing::{
        TestContext,
        oracle::{PHP_PREAMBLE, compare, encode_case, find_php, php_literal},
    },
};

pub type Case = (&'static str, Vec<Value>);

pub async fn check(name: &str, cases: &[Case], oracle_cwd: &Path, native_cwd: &Path, setup: &str) {
    let php = find_php().expect("PHP 8.5 is required for the streams oracle");
    let script = tempfile::NamedTempFile::new().expect("create oracle PHP script");
    let mut source = format!("<?php\n{PHP_PREAMBLE}\n{setup}\n");
    let mut labels = Vec::with_capacity(cases.len());
    for (index, (function, arguments)) in cases.iter().enumerate() {
        let arguments = arguments
            .iter()
            .map(php_literal)
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!("{function}({arguments})");
        source.push_str(&format!("run('{index}', static fn() => {call});\n"));
        labels.push(call);
    }
    fs::write(script.path(), source).expect("write generated PHP oracle inputs");
    let output = Command::new(php)
        .args(["-n", "-d", "error_reporting=-1"])
        .arg(script.path())
        .current_dir(oracle_cwd)
        .output()
        .expect("run PHP oracle");
    assert!(
        output.status.success(),
        "PHP oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let mut registry = Registry::new();
    registry.load(&Streams);
    let mut context = TestContext::default();
    set_request(
        &mut context.state,
        Some(native_cwd.to_owned()),
        Default::default(),
    );
    let mut actual = Vec::with_capacity(cases.len());
    for (index, (function, arguments)) in cases.iter().enumerate() {
        let mut arguments = arguments.clone();
        let entry = registry
            .get_function(function)
            .expect("registered stream function");
        let result = match prepare_arguments(entry, &mut arguments, &mut context) {
            Ok(()) => match entry.handler {
                NativeFn::Sync(handler) | NativeFn::DeprecatedSync(handler) => {
                    handler(&mut context, &arguments)
                }
                NativeFn::Async(handler) => handler(&mut context, arguments).await,
            },
            Err(error) => Err(error),
        };
        actual.push(encode_case(
            &index.to_string(),
            &result,
            &context.output,
            &context.diagnostics,
        ));
        context.output.clear();
        context.diagnostics.entries.clear();
    }
    eprintln!("{} compared calls for {name}", cases.len());
    compare(name, &labels, &oracle, &actual);
}
