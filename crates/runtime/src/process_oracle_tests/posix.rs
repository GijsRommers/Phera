//! `posix_getpwuid(posix_getuid())` uses the same account database as PHP on this machine.

use super::Cases;

#[tokio::test(flavor = "current_thread")]
async fn posix_queries_match_php() {
    let mut cases = Cases::new();
    for expression in [
        "posix_getpid() > 0",
        "posix_getppid() >= 0",
        "posix_getuid()",
        "posix_geteuid()",
        "posix_getgid()",
        "posix_getegid()",
        "posix_getcwd()",
        "posix_uname()",
        "posix_get_last_error()",
        "array_keys(posix_times())",
        "array_map('gettype', posix_times())",
        "posix_getpwuid(posix_getuid())",
        "posix_getpwnam(posix_getpwuid(posix_getuid())['name'])",
        "posix_getgrgid(posix_getgid())",
        "posix_kill(posix_getpid(), 0)",
    ] {
        cases.result(expression.into());
    }
    for function in [
        "posix_getpid",
        "posix_getppid",
        "posix_getuid",
        "posix_geteuid",
        "posix_getgid",
        "posix_getegid",
        "posix_getcwd",
        "posix_uname",
        "posix_times",
        "posix_get_last_error",
    ] {
        cases.result(format!("{function}(1)"));
    }
    for function in ["posix_isatty", "posix_ttyname", "stream_isatty"] {
        for input in [
            "null",
            "false",
            "true",
            "-1",
            "0",
            "1",
            "2",
            "67108864",
            "PHP_INT_MAX",
            "PHP_INT_MIN",
            "[]",
            "[1]",
            "new stdClass",
            "'1.6'",
            "1.6",
            "'foo'",
            "fopen('php://memory', 'w+')",
            "fopen('/dev/null', 'r')",
            "INF",
        ] {
            cases.check(format!(
                "$value = {function}({input}); return [$value, posix_get_last_error()];"
            ));
        }
    }
    for input in [
        "null",
        "false",
        "true",
        "-1",
        "0",
        "1",
        "2",
        "67108864",
        "PHP_INT_MAX",
        "PHP_INT_MIN",
        "[]",
        "[1]",
        "new stdClass",
        "'1.6'",
        "1.6",
        "'foo'",
        "INF",
    ] {
        for function in ["posix_getpwuid", "posix_getgrgid"] {
            cases.check(format!(
                "$value = {function}({input}); return [$value, posix_get_last_error()];"
            ));
        }
    }
    for input in [
        "null",
        "false",
        "true",
        "''",
        "'root'",
        "'phera-absent-user'",
        "[]",
        "new stdClass",
        "\"root\\0suffix\"",
        "hex2bin('ff')",
    ] {
        cases.check(format!(
            "$value = posix_getpwnam({input}); return [$value, posix_get_last_error()];"
        ));
    }
    for number in -256..=1023 {
        cases.result(format!("posix_strerror({number})"));
    }
    for filename in [
        "''",
        "'/'",
        "'/dev/null'",
        "'/phera-absent-file'",
        "\"bad\\0name\"",
        "[]",
    ] {
        for flags in [
            "null",
            "0",
            "1",
            "2",
            "4",
            "7",
            "8",
            "-1",
            "PHP_INT_MAX",
            "[]",
        ] {
            cases.check(format!(
                "$value = posix_access({filename}, {flags}); return [$value, posix_get_last_error()];"
            ));
        }
    }
    for pid in ["getmypid()", "0", "PHP_INT_MAX", "67108864"] {
        for signal in ["0", "-1", "9999", "PHP_INT_MAX"] {
            cases.check(format!(
                "$value = posix_kill({pid}, {signal}); return [$value, posix_get_last_error()];"
            ));
        }
    }
    for mode in ["0600", "0666", "0777", "0000", "null", "[]"] {
        cases.check(format!(
            "$path = tempnam(sys_get_temp_dir(), 'phera-fifo'); unlink($path);              try {{ $value = posix_mkfifo($path, {mode});                 return [$value, posix_mkfifo($path, {mode}), posix_get_last_error()];              }} finally {{ if (file_exists($path)) unlink($path); }}"
        ));
    }
    cases.compare("unix-posix-queries").await;
}
