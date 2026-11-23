//! Comparing native functions against real PHP.
//!
//! An oracle test builds a list of cases, writes one PHP script that runs
//! them all and prints one line per case, runs `php` once, then produces the
//! same lines from Rust and compares. Both sides use the encoding defined
//! here, which keeps every distinction PHP makes: int against float, `-0.0`
//! against `0.0`, bytes that are not UTF-8, key order.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use phera_value::{ArrayKey, CollectDiagnostics, DiagnosticKind, Value, float_repr};

use crate::{NativeError, NativeResult};

/// PHP code that defines `enc($value)`, collects warnings, notices and
/// deprecations, and offers `run($tag, $closure)` to print one case line:
/// `tag=<value or error>|<diagnostic>|...`.
pub const PHP_PREAMBLE: &str = r#"
$ERRBUF = [];
set_error_handler(function ($no, $msg) {
    $GLOBALS['ERRBUF'][] = match ($no) {
        2, 512 => 'W:',
        8, 1024 => 'N:',
        8192, 16384 => 'D:',
        default => "X$no:",
    } . $msg;
    return true;
});
function enck($k) { return is_int($k) ? "I:$k" : 'S:' . bin2hex($k); }
function encf($f) {
    if (is_nan($f)) return 'F:NAN';
    return 'F:' . bin2hex(pack('E', $f));
}
function enc($v) {
    if (is_object($v)) {
        $s = 'O:' . get_class($v) . '(';
        foreach ((array) $v as $k => $pv) { $s .= enck($k) . '>' . enc($pv) . ';'; }
        return $s . ')';
    }
    if (is_array($v)) {
        $s = 'A(';
        foreach ($v as $k => $pv) { $s .= enck($k) . '>' . enc($pv) . ';'; }
        return $s . ')';
    }
    if (is_int($v)) return "I:$v";
    if (is_float($v)) return encf($v);
    if (is_string($v)) return 'S:' . bin2hex($v);
    if (is_bool($v)) return $v ? 'B:1' : 'B:0';
    if (is_null($v)) return 'N';
    return 'OTHER';
}
function run($tag, $f) {
    global $ERRBUF;
    $ERRBUF = [];
    ob_start();
    try {
        $s = enc($f());
    } catch (Throwable $e) {
        $s = 'E:' . get_class($e) . ':' . $e->getMessage();
    }
    $out = ob_get_clean();
    $line = $tag . '=' . $s;
    if ($out !== '') { $line .= '|O:' . bin2hex($out); }
    foreach ($ERRBUF as $w) { $line .= '|' . str_replace("\n", '\n', $w); }
    echo $line, "\n";
}
"#;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Encodes IEEE 754 bits in big-endian hex, canonicalizing only NaNs.
pub fn encode_float(float: f64) -> String {
    if float.is_nan() {
        "F:NAN".to_string()
    } else {
        format!("F:{:016x}", float.to_bits())
    }
}

/// Checks two untagged encoded float results within one ULP. Callers must
/// explicitly opt in for platform-libm functions; `compare` remains exact.
/// Output, reference mutations and diagnostics after the result stay exact.
pub fn float_results_within_one_ulp(php: &str, rust: &str) -> bool {
    fn decode(line: &str) -> Option<(f64, &str)> {
        let (result, rest) = line.split_at(line.find('|').unwrap_or(line.len()));
        let bits = result.strip_prefix("F:")?;
        if bits.len() != 16 {
            return None;
        }
        Some((f64::from_bits(u64::from_str_radix(bits, 16).ok()?), rest))
    }

    let (Some((php, php_rest)), Some((rust, rust_rest))) = (decode(php), decode(rust)) else {
        return false;
    };
    // Within one sign, IEEE 754 bit patterns are monotonic (reversed for
    // negatives), so their absolute integer distance is the ULP distance.
    php_rest == rust_rest
        && php.is_finite()
        && rust.is_finite()
        && php.is_sign_negative() == rust.is_sign_negative()
        && php.to_bits().abs_diff(rust.to_bits()) <= 1
}

fn encode_key(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(int) => format!("I:{int}"),
        ArrayKey::String(string) => format!("S:{}", hex(string.as_bytes())),
    }
}

/// The Rust counterpart of the PHP function `enc`.
pub fn encode(value: &Value) -> String {
    match value {
        Value::Null => "N".to_string(),
        Value::Bool(flag) => format!("B:{}", u8::from(*flag)),
        Value::Int(int) => format!("I:{int}"),
        Value::Float(float) => encode_float(*float),
        Value::String(string) => format!("S:{}", hex(string.as_bytes())),
        Value::Array(array) => {
            let mut text = String::from("A(");
            for (key, item) in array.iter() {
                text.push_str(&format!("{}>{};", encode_key(&key), encode(item)));
            }
            text + ")"
        }
        Value::Object(object) => {
            let mut text = format!("O:{}(", object.class.name());
            for (key, item) in object.to_array().iter() {
                text.push_str(&format!("{}>{};", encode_key(&key), encode(item)));
            }
            text + ")"
        }
        Value::Resource(_) => "OTHER".to_string(),
        Value::Reference(_) => encode(&value.deref()),
    }
}

/// One case line as the PHP function `run` prints it: the result or the
/// thrown error, then what the function wrote, then its diagnostics.
pub fn encode_case(
    tag: &str,
    result: &NativeResult,
    output: &[u8],
    diagnostics: &CollectDiagnostics,
) -> String {
    let mut line = match result {
        Ok(value) => format!("{tag}={}", encode(value)),
        Err(NativeError::Throw { class, message, .. }) => format!("{tag}=E:{class}:{message}"),
        Err(NativeError::Unwind(_)) => format!("{tag}=UNWIND"),
    };
    if !output.is_empty() {
        line.push_str(&format!("|O:{}", hex(output)));
    }
    for (kind, message) in &diagnostics.entries {
        let kind = match kind {
            DiagnosticKind::Warning => "W:",
            DiagnosticKind::Notice => "N:",
            DiagnosticKind::Deprecated => "D:",
        };
        line.push_str(&format!("|{kind}{}", message.replace('\n', "\\n")));
    }
    line
}

/// PHP source that evaluates to `value`. Objects and resources have no
/// literal; build those in the script by hand.
pub fn php_literal(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Int(i64::MIN) => "PHP_INT_MIN".to_string(),
        Value::Int(int) => int.to_string(),
        Value::Float(float) if float.is_nan() => "NAN".to_string(),
        Value::Float(float) if float.is_infinite() => {
            if *float > 0.0 { "INF" } else { "-INF" }.to_string()
        }
        // The shortest representation reads back as the same float, and the
        // cast keeps `1` a float and `-0` negative.
        Value::Float(float) => format!("(float)\"{}\"", float_repr(*float)),
        Value::String(string) => {
            let escaped: String = string
                .as_bytes()
                .iter()
                .map(|byte| format!("\\x{byte:02x}"))
                .collect();
            format!("\"{escaped}\"")
        }
        Value::Array(array) => {
            let items: Vec<String> = array
                .iter()
                .map(|(key, item)| {
                    let key = match key {
                        ArrayKey::Int(int) => php_literal(&Value::Int(int)),
                        ArrayKey::String(string) => php_literal(&Value::String(string.clone())),
                    };
                    format!("{key} => {}", php_literal(item))
                })
                .collect();
            format!("[{}]", items.join(", "))
        }
        Value::Reference(_) => php_literal(&value.deref()),
        Value::Object(_) | Value::Resource(_) => {
            panic!("php_literal: objects and resources have no literal")
        }
    }
}

/// Resolves the shared oracle binary: `PHERA_PHP`, Homebrew, then `PATH`.
pub fn find_php() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PHERA_PHP").map(PathBuf::from)
        && path.exists()
    {
        return Some(path);
    }
    let homebrew = PathBuf::from("/opt/homebrew/bin/php");
    if homebrew.exists() {
        return Some(homebrew);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join("php"))
        .find(|candidate| candidate.is_file())
}

/// The oracle's `PHP_VERSION_ID`, queried once per test process.
pub fn php_version_id() -> Option<u32> {
    static VERSION_ID: OnceLock<Option<u32>> = OnceLock::new();
    *VERSION_ID.get_or_init(|| {
        let php = find_php()?;
        let output = Command::new(php)
            .args(["-n", "-r", "echo PHP_VERSION_ID;"])
            .output()
            .expect("query oracle PHP_VERSION_ID");
        assert!(output.status.success(), "query oracle PHP_VERSION_ID");
        Some(
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse()
                .expect("numeric oracle PHP_VERSION_ID"),
        )
    })
}

/// Whether the oracle can check behavior introduced in a given PHP patch.
pub fn supports_php_since(case: &str, minimum: u32) -> bool {
    let version = php_version_id().expect("PHP oracle available");
    if version >= minimum {
        return true;
    }
    oracle_status(format_args!(
        "SKIP {case}: PHP behaviour since {}; oracle is {}",
        display_version(minimum),
        display_version(version)
    ));
    false
}

pub(super) fn oracle_status(message: std::fmt::Arguments<'_>) {
    // Direct writes keep version skips visible under cargo test's output capture.
    writeln!(io::stderr().lock(), "{message}").expect("write oracle version status");
}

fn display_version(id: u32) -> String {
    format!("{}.{}.{}", id / 10_000, id / 100 % 100, id % 100)
}

/// Source path that PHP reports for a diagnostic raised in the oracle script.
pub fn script_path(name: &str) -> PathBuf {
    let tmp = std::env::temp_dir();
    let directory = tmp.canonicalize().unwrap_or(tmp).join("phera-oracle");
    directory.join(format!("{name}.php"))
}

/// Runs `script` (which must start with `<?php`) and returns its output
/// lines, or `None` when no `php` binary is installed, in which case the
/// caller skips its test. `name` keeps the script files of different tests
/// apart.
pub fn run_php(name: &str, script: &str) -> Option<Vec<String>> {
    let php = find_php()?;
    let file = script_path(name);
    std::fs::create_dir_all(file.parent().expect("oracle directory"))
        .expect("create oracle directory");
    std::fs::write(&file, script).expect("write oracle script");
    let output = Command::new(&php)
        .args(["-n", "-d", "error_reporting=-1", "-d", "memory_limit=1G"])
        .arg(&file)
        .output()
        .expect("run php");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "php exited with {:?}\nstderr:\n{}\nstdout starts:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        stdout.chars().take(2000).collect::<String>()
    );
    Some(stdout.lines().map(str::to_string).collect())
}

/// Fails the test with the first 30 differing lines, each labelled with the
/// case that produced it.
pub fn compare(what: &str, labels: &[String], php: &[String], rust: &[String]) {
    assert_eq!(
        php.len(),
        rust.len(),
        "{what}: PHP and Rust produced a different number of lines"
    );
    let mismatches: Vec<String> = php
        .iter()
        .zip(rust)
        .enumerate()
        .filter(|(_, (php, rust))| php != rust)
        .map(|(index, (php, rust))| {
            let label = labels.get(index).map_or("", String::as_str);
            format!("#{index} {label}\n  php : {php}\n  rust: {rust}")
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{what}: {} mismatches out of {}\n{}",
        mismatches.len(),
        php.len(),
        mismatches
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    println!("{what}: {} cases, zero mismatches", php.len());
}

#[cfg(test)]
mod tests {
    use super::{encode_float, float_results_within_one_ulp};

    #[test]
    fn one_ulp_requires_finite_floats_with_the_same_sign() {
        for number in [0.5_f64, -0.5, f64::MIN_POSITIVE, -f64::MIN_POSITIVE] {
            let encoded = encode_float(number);
            let adjacent = encode_float(f64::from_bits(number.to_bits() + 1));
            let farther = encode_float(f64::from_bits(number.to_bits() + 2));
            assert!(float_results_within_one_ulp(&encoded, &adjacent));
            assert!(float_results_within_one_ulp(&adjacent, &encoded));
            assert!(!float_results_within_one_ulp(&encoded, &farther));
        }
        assert!(!float_results_within_one_ulp(
            &encode_float(0.0),
            &encode_float(-0.0)
        ));
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(!float_results_within_one_ulp(
                &encode_float(number),
                &encode_float(number)
            ));
            assert!(!float_results_within_one_ulp(
                &encode_float(number),
                &encode_float(f64::MAX)
            ));
        }
    }

    #[test]
    fn one_ulp_keeps_other_values_and_diagnostics_exact() {
        let php = "F:3fe193ea7aad030b|2:7761726e696e67|O:";
        let rust = "F:3fe193ea7aad030a|2:7761726e696e67|O:";
        assert!(float_results_within_one_ulp(php, rust));
        assert!(!float_results_within_one_ulp(php, "F:3fe193ea7aad030a|O:"));
        assert!(!float_results_within_one_ulp(
            php,
            "F:3fe193ea7aad030a|2:7761726e696e67|O:78"
        ));
        for value in ["I:1", "S:31", "N", "B:1", "A(I:0>F:3fe193ea7aad030a;)"] {
            assert!(!float_results_within_one_ulp(php, value));
        }
    }
}
