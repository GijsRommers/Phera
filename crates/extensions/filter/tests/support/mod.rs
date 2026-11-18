//! Compare output and diagnostics in the same PHP program on both runtimes.

use std::fmt::Write as _;
use std::path::PathBuf;

use phera_extension_api::testing::oracle;
use phera_extension_api::{ArrayKey, PhpArray, Value};
use phera_runtime::Runtime;

pub mod inputs;

mod byte_cases;
mod option_cases;
mod tree_cases;

const HARNESS: &str = r#"<?php
error_reporting(-1);
set_error_handler(function ($level, $message) {
    echo 'D', $level, ':', bin2hex($message), '|';
    return true;
});
function compare_case($number, $operation) {
    echo $number, ':';
    try {
        $result = $operation();
        echo 'V:', bin2hex(var_export($result, true));
    } catch (Throwable $error) {
        echo 'E:', bin2hex(get_class($error)), ':', bin2hex($error->getMessage()),
            ':', $error->getCode();
    }
    echo "\n";
}
function roundtrip($input) { return [$input, strlen($input)]; }
function failure($input) { throw new RuntimeException("callback: " . $input, 12); }
class FilterString {
    public function __toString(): string { return "7"; }
}
class FilterName {
    public function __toString(): string { return "int"; }
}
class FilterCallback {
    public function __invoke($input) { return $input . "!"; }
    public static function length($input) { return strlen($input); }
}
"#;

#[derive(Default)]
pub struct Matrix {
    labels: Vec<String>,
}

impl Matrix {
    pub fn call(&mut self, function: &str, inputs: &[Value]) {
        let inputs = inputs.iter().map(oracle::php_literal).collect::<Vec<_>>();
        self.expression(format!("{function}({})", inputs.join(", ")));
    }

    pub fn expression(&mut self, expression: impl Into<String>) {
        self.labels.push(expression.into());
    }

    pub fn run(self, name: &str, prelude: &str) {
        let script = self.script(prelude);
        let Some(php) = oracle::run_php(name, &script) else {
            eprintln!("skipping {name}: no PHP oracle found");
            return;
        };
        let file = phera_file(name);
        std::fs::write(&file, script).expect("write Phera oracle script");
        let result = Runtime::new()
            .run_file_blocking(&file, &[], None)
            .expect("run Phera");
        let output = String::from_utf8(result.output).expect("ASCII test output");
        assert_eq!(result.status, 0, "{name} failed: {output}");
        let lines = output.lines().map(str::to_owned).collect::<Vec<_>>();
        oracle::compare(name, &self.labels, &php, &lines);
    }

    fn script(&self, prelude: &str) -> String {
        let mut script = String::from("<?php\n");
        script.push_str(prelude);
        script.push_str(HARNESS.strip_prefix("<?php").expect("PHP harness"));
        // Keep each generated function below the compiler's register limit.
        const BATCH_SIZE: usize = 1000;
        for (batch, expressions) in self.labels.chunks(BATCH_SIZE).enumerate() {
            writeln!(script, "function filter_batch_{batch}() {{").unwrap();
            for (offset, expression) in expressions.iter().enumerate() {
                let index = batch * BATCH_SIZE + offset;
                writeln!(script, "compare_case({index}, fn() => {expression});").unwrap();
            }
            writeln!(
                script,
                "}}
filter_batch_{batch}();"
            )
            .unwrap();
        }
        script
    }
}

fn phera_file(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("phera-oracle");
    std::fs::create_dir_all(&directory).expect("create Phera oracle directory");
    directory.join(format!("{name}-phera.php"))
}

pub fn associative(entries: &[(&str, Value)]) -> Value {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(ArrayKey::str(*key), value.clone());
    }
    Value::from(array)
}

pub fn list(entries: &[Value]) -> Value {
    let mut array = PhpArray::new();
    for value in entries {
        array.push(value.clone()).expect("small test array");
    }
    Value::from(array)
}

pub fn options(flags: i64, entries: &[(&str, Value)]) -> Value {
    associative(&[
        ("flags", Value::Int(flags)),
        ("options", associative(entries)),
    ])
}
