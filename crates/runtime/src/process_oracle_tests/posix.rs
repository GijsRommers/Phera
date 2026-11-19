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
    for name in [
        "POSIX_F_OK",
        "POSIX_R_OK",
        "POSIX_W_OK",
        "POSIX_X_OK",
        "POSIX_S_IFREG",
        "POSIX_S_IFCHR",
        "POSIX_S_IFBLK",
        "POSIX_S_IFIFO",
        "POSIX_S_IFSOCK",
        "POSIX_RLIMIT_AS",
        "POSIX_RLIMIT_CORE",
        "POSIX_RLIMIT_CPU",
        "POSIX_RLIMIT_DATA",
        "POSIX_RLIMIT_FSIZE",
        "POSIX_RLIMIT_LOCKS",
        "POSIX_RLIMIT_MEMLOCK",
        "POSIX_RLIMIT_MSGQUEUE",
        "POSIX_RLIMIT_NICE",
        "POSIX_RLIMIT_NOFILE",
        "POSIX_RLIMIT_NPROC",
        "POSIX_RLIMIT_RSS",
        "POSIX_RLIMIT_RTPRIO",
        "POSIX_RLIMIT_RTTIME",
        "POSIX_RLIMIT_SIGPENDING",
        "POSIX_RLIMIT_STACK",
        "POSIX_RLIMIT_KQUEUES",
        "POSIX_RLIMIT_NPTS",
        "POSIX_RLIMIT_INFINITY",
        "POSIX_SC_ARG_MAX",
        "POSIX_SC_CHILD_MAX",
        "POSIX_SC_CLK_TCK",
        "POSIX_SC_PAGESIZE",
        "POSIX_SC_NPROCESSORS_CONF",
        "POSIX_SC_NPROCESSORS_ONLN",
        "POSIX_SC_OPEN_MAX",
        "POSIX_PC_LINK_MAX",
        "POSIX_PC_MAX_CANON",
        "POSIX_PC_MAX_INPUT",
        "POSIX_PC_NAME_MAX",
        "POSIX_PC_PATH_MAX",
        "POSIX_PC_PIPE_BUF",
        "POSIX_PC_CHOWN_RESTRICTED",
        "POSIX_PC_NO_TRUNC",
        "POSIX_PC_ALLOC_SIZE_MIN",
        "POSIX_PC_SYMLINK_MAX",
    ] {
        cases.result(format!("defined('{name}') ? constant('{name}') : null"));
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
                "$value = posix_access({filename}, {flags}); \
                 return [$value, posix_get_last_error()];"
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
    for overflow in ["PHP_INT_MAX", "PHP_INT_MIN"] {
        cases.result(format!("posix_kill({overflow}, 15)"));
    }
    for mode in ["0600", "0666", "0777", "0000", "null", "[]"] {
        cases.check(format!(
            "$path = tempnam(sys_get_temp_dir(), 'phera-fifo'); unlink($path); \
             try {{ $value = posix_mkfifo($path, {mode}); \
             return [$value, posix_mkfifo($path, {mode}), posix_get_last_error()]; \
             }} finally {{ if (file_exists($path)) unlink($path); }}"
        ));
    }
    cases.compare("unix-posix-queries").await;
}
