//! The same PHP file under two binaries, including return types and exact diagnostics.

mod support;

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use phera_extension_api::testing::oracle;
use support::Project;

fn compare(php: &Path, file: &Path, case: usize) {
    let run = |program: &OsStr, flag: &str| {
        let mut cmd = Command::new(program);
        cmd.env_clear().envs([
            ("__CF_USER_TEXT_ENCODING", "controlled"),
            ("BASE", "original"),
            ("base", "lower"),
            ("42", "numeric"),
            ("A B", "spaced"),
            ("A.B", "dotted"),
            ("A[B]", "bracketed"),
            ("HTTP_PROXY", "fixture"),
            ("DOCUMENT_ROOT", "shadow"),
            ("PHP_SELF", "shadow"),
        ]);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            cmd.env(
                OsStr::from_bytes(b"raw\xff"),
                OsStr::from_bytes(b"\xfe=data"),
            );
        }
        cmd.arg(flag).arg(file).arg("argument").output().unwrap()
    };
    let oracle = run(php.as_os_str(), "-n");
    let phera = run(OsStr::new(env!("CARGO_BIN_EXE_phera")), "--no-env-file");
    assert!(oracle.status.success(), "PHP failed in group {case}");
    assert_eq!(
        phera.status.code(),
        oracle.status.code(),
        "exit, group {case}"
    );
    assert_eq!(phera.stderr, oracle.stderr, "stderr, group {case}");
    if phera.stdout != oracle.stdout {
        let offset = phera
            .stdout
            .iter()
            .zip(&oracle.stdout)
            .position(|(a, b)| a != b)
            .unwrap_or(phera.stdout.len().min(oracle.stdout.len()));
        let describe = |s: &[u8]| {
            String::from_utf8_lossy(&s[offset.saturating_sub(60)..s.len().min(offset + 120)])
                .into_owned()
        };
        panic!(
            "group {case}, byte {offset}\nPHP {:?}\nPhera {:?}",
            describe(&oracle.stdout),
            describe(&phera.stdout)
        );
    }
}

fn observe(call: &str) -> String {
    format!(
        "try {{ var_dump({call}); }} catch (Throwable $e) {{ echo get_class($e), \
        '|', $e->getMessage(), \"\\n\"; }}\n"
    )
}

#[test]
fn cli_environment_matches_php() {
    let oracle = oracle::find_php().expect("PHP 8.5 required");
    // Resolve PHP_BINARY before clearing the environment. CI's oracle wrapper
    // needs its own configuration and Bash adds variables when it starts.
    let real = Command::new(oracle)
        .args(["-n", "-r", "echo PHP_BINARY;"])
        .output()
        .unwrap();
    assert!(real.status.success());
    let executable = String::from_utf8(real.stdout).unwrap();
    let php = Path::new(&executable);
    let project = Project::new();
    let mut calls = 0;
    let names = [
        "'BASE'",
        "'base'",
        "'new'",
        "'A B'",
        "'A.B'",
        "'A[B]'",
        "'0'",
        "'42'",
        "'=bad'",
        "''",
        "'BASE=original'",
        "'raw'.chr(255)",
        "'BASE'.chr(0).'z'",
    ];
    for (case, name) in names.iter().enumerate() {
        let mut script = String::from("<?php\n");
        for setting in [
            "'one'",
            "''",
            "'a=b c'",
            "chr(254)",
            "'pre'.chr(0).'post'",
            "str_repeat('q', 16384)",
        ] {
            let expressions = [
                format!("putenv({name}.'='.{setting})"),
                format!("getenv({name})"),
                format!("getenv({name}, true)"),
                format!("getenv({name}, false)"),
                "getenv()".into(),
                format!("putenv({name})"),
                format!("getenv({name})"),
                format!("$_ENV[{name}] ?? null"),
                format!("$_SERVER[{name}] ?? null"),
            ];
            for expression in expressions {
                calls += 1;
                script.push_str(&observe(&expression));
            }
        }
        let file = project.write("probe.php", script);
        compare(php, &file, case);
    }
    let mut script = String::from("<?php\n");
    for value in [
        "null",
        "true",
        "false",
        "0",
        "-1",
        "1.5",
        "[]",
        "new stdClass()",
        "''",
    ] {
        for call in [
            format!("getenv({value})"),
            format!("getenv(null,{value})"),
            format!("putenv({value})"),
        ] {
            script.push_str(&observe(&call));
            calls += 1;
        }
    }
    for call in [
        "getenv(null,true,0)",
        "putenv()",
        "putenv('x','y')",
        "getenv(local_only:true)",
        "putenv(assignment:'new=value')",
        "getenv(name:'new')",
        "putenv('BASE')",
        "putenv('other=x')",
        "putenv('BASE=back')",
        "putenv('other=y')",
        "getenv()",
        "$_ENV",
    ] {
        script.push_str(&observe(call));
        calls += 1;
    }
    script.push_str("unset($_SERVER['REQUEST_TIME'], $_SERVER['REQUEST_TIME_FLOAT']);\n");
    script.push_str("var_dump($_SERVER);\n");
    compare(php, &project.write("probe.php", script), names.len());
    for strict in [false, true] {
        let mut script = if strict {
            "<?php declare(strict_types=1);\n"
        } else {
            "<?php\n"
        }
        .to_owned();
        script.push_str("class Key { function __toString(): string { return 'BASE'; } }\n");
        script.push_str("class Bad { function __toString() { return []; } }\n");
        script
            .push_str("class Thrown { function __toString() { throw new Exception('cast'); } }\n");
        for expr in [
            "getenv(new Key())",
            "putenv(new Key())",
            "getenv(new Key(),[])",
            "getenv(new Bad())",
            "putenv(new Thrown())",
            "putenv(new Key(),1)",
            "getenv('BASE')",
        ] {
            script.push_str(&observe(expr));
            calls += 1;
        }
        compare(
            php,
            &project.write("cast.php", script),
            14 + usize::from(strict),
        );
    }
    // Exercise the ordering of inherited, removed, restored and request-created names.
    let mut state = 17u32;
    for group in 0..16 {
        let mut script = String::from("<?php\n");
        for _ in 0..32 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let name = ["BASE", "base", "42", "fresh", "other"][(state as usize >> 8) % 5];
            let suffix = ["", "=", "=x", "=a=b"][(state >> 16) as usize % 4];
            script.push_str(&format!(
                "var_dump(putenv('{name}{suffix}'),getenv(),getenv('{name}'));\n"
            ));
            calls += 3;
        }
        compare(php, &project.write("order.php", script), 16 + group);
    }
    eprintln!("{} PHP environment comparisons", calls + 1);
}
