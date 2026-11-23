//! Exercise startup assertion settings through the CLI.

use std::process::Command;

#[test]
fn assertion_startup_and_runtime_settings() {
    let cases = [
        (
            "zend.assertions=-1",
            r#"<?php
function assertion_argument(): bool {
    echo "evaluated\n";
    return false;
}

set_error_handler(function ($level, $message) { echo $message, "\n"; });
var_dump(ini_get('zend.assertions'));
var_dump(assert(assertion_argument()));
var_dump(ini_set('zend.assertions', '0'));
var_dump(ini_set('zend.assertions', '1'));
var_dump(ini_set('zend.assertions', '-1'));
$fn = 'assert';
var_dump($fn(assertion_argument()));
var_dump(call_user_func('assert', assertion_argument()));
"#,
            r#"string(2) "-1"
bool(true)
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
string(2) "-1"
evaluated
bool(true)
evaluated
bool(true)
"#,
        ),
        (
            "zend.assertions=0",
            r#"<?php
namespace Parts {
    function probe(): bool {
        echo "ordinary function\n";
        return false;
    }
}

namespace App {
    use function Parts\probe as assert;
    use function \assert as check;

    $n = 0;
    var_dump(\assert(++$n), check(++$n), $n);
    var_dump(assert());
    var_dump(ini_set('zend.assertions', '1'));

    try { check($n > 0); }
    catch (\AssertionError $e) { echo $e->getMessage(), "\n"; }

    try { \assert(false, new \LogicException('direct')); }
    catch (\LogicException $e) { echo $e->getMessage(), "\n"; }

    try { \assert(true, new \stdClass); }
    catch (\TypeError $e) { echo $e->getMessage(), "\n"; }

    try { \assert(false, new class { function __toString() { return 'converted'; } }); }
    catch (\AssertionError $e) { echo $e->getMessage(), "\n"; }

    set_error_handler(function ($level, $message) { echo $message, "\n"; });
    var_dump(ini_set('zend.assertions', '-1'));
    ini_restore('zend.assertions');
    var_dump(\assert(++$n), $n);
}
"#,
            r#"bool(true)
bool(true)
int(0)
ordinary function
bool(false)
string(1) "0"
assert($n > 0)
direct
assert(): Argument #2 ($description) must be of type Throwable|string|null, stdClass given
converted
zend.assertions may be completely enabled or disabled only in php.ini
bool(false)
bool(true)
int(0)
"#,
        ),
    ];
    let directory = tempfile::tempdir().unwrap();
    let entry = directory.path().join("assertions.php");
    for (setting, script, expected) in cases {
        std::fs::write(&entry, script).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_phera"))
            .env_clear()
            .envs(
                ["PATH", "HOME", "TMPDIR"]
                    .into_iter()
                    .filter_map(|name| std::env::var_os(name).map(|value| (name, value))),
            )
            .args(["--no-env-file", "-d", setting])
            .arg(&entry)
            .output()
            .unwrap();
        assert!(output.status.success(), "{setting}: {:?}", output.status);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{setting}"
        );
    }
}
