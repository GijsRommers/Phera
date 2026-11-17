//! Differential observations include return, references, diagnostics and last error.

mod callbacks;
mod corpus;
mod edges;
pub use callbacks::Harness;
pub use corpus::{patterns, subjects};
pub use edges::cases as edge_cases;
use phera_extension_api::testing::oracle;
use phera_extension_api::{ArrayKey, NativeError, PhpArray, Value};
use std::cell::RefCell;
use std::io::Write;
use std::process::{Command, Stdio};
use std::rc::Rc;

pub fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(i, value)| (ArrayKey::Int(i as i64), value))
            .collect::<PhpArray>(),
    )
}
pub fn reference() -> Value {
    Value::Reference(Rc::new(RefCell::new(Value::from("untouched"))))
}

pub struct Matrix {
    pub harness: Harness,
    script: String,
    labels: Vec<String>,
    rust: Vec<String>,
}
impl Matrix {
    pub fn new() -> Self {
        let functions = r#"
function upper($m) { return strtoupper(is_array($m[0]) ? $m[0][0] : $m[0]); }
function reverse($m) { return strrev(is_array($m[0]) ? $m[0][0] : $m[0]); }
function length($m) { return strlen(is_array($m[0]) ? $m[0][0] : $m[0]); }
function identity($m) { return $m[0]; }
function fail($m) { throw new ValueError('callback failed'); }
function null_result($m) { return null; }
function array_result($m) { return []; }
function inspect($m) { return enc($m); }
"#;
        Self {
            harness: Harness::default(),
            script: format!("<?php\n{}\n{functions}\n", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            rust: Vec::new(),
        }
    }
    pub fn case(&mut self, name: &str, arguments: Vec<Value>) {
        let mut setup = String::new();
        let mut refs = Vec::new();
        let mut literals = Vec::new();
        for (index, value) in arguments.iter().enumerate() {
            if value.is_reference() {
                setup.push_str(&format!("$ref{index} = {};", oracle::php_literal(value)));
                refs.push(format!("$ref{index}"));
                literals.push(format!("$ref{index}"));
            } else {
                literals.push(oracle::php_literal(value));
            }
        }
        let expression = format!("{name}({})", literals.join(","));
        // Capture exception details as values so refs and error state are checked even on throws.
        self.script.push_str(&format!(
            "run('case', function() {{ {setup} try {{ $r = {expression}; $e = null; }} \
             catch (Throwable $t) {{ $r = null; $e = [get_class($t), $t->getMessage()]; }} \
             return [$r, $e, [{}], preg_last_error(), preg_last_error_msg()]; }});\n",
            refs.join(",")
        ));
        self.harness.base.output.clear();
        self.harness.base.diagnostics.entries.clear();
        let result = self.harness.run(name, arguments.clone());
        let (value, exception) = match result {
            Ok(value) => (value, Value::Null),
            Err(NativeError::Throw { class, message, .. }) => (
                Value::Null,
                list([Value::from(class), Value::from(message)]),
            ),
            Err(error) => panic!("unexpected unwind: {error:?}"),
        };
        let refs = list(
            arguments
                .iter()
                .filter(|value| value.is_reference())
                .map(|value| value.deref().clone()),
        );
        let code = self.harness.run("preg_last_error", vec![]).unwrap();
        let message = self.harness.run("preg_last_error_msg", vec![]).unwrap();
        self.rust.push(oracle::encode_case(
            "case",
            &Ok(list([value, exception, refs, code, message])),
            &self.harness.base.output,
            &self.harness.base.diagnostics,
        ));
        self.labels.push(expression.chars().take(550).collect());
    }
    pub fn compare(self, name: &str) {
        // Feed stdin on a separate writer so large scripts/output cannot fill both pipes.
        // No temporary artifacts outside the task's assigned scratch directory.
        let mut child = Command::new(oracle::find_php().expect("PHP 8.5 oracle required"))
            .args(["-n", "-d", "memory_limit=1G", "-d", "error_reporting=-1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("PHP 8.5 oracle required");
        let mut stdin = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(self.script.as_bytes()).unwrap());
        let output = child.wait_with_output().unwrap();
        writer.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let php: Vec<String> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        oracle::compare(name, &self.labels, &php, &self.rust);
    }
}
