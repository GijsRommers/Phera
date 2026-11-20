//! Unclosed popen streams send EOF and reap their children before the CLI returns.

mod support;

use std::fs;
use std::process::Command;

use phera_extension_api::testing::oracle;
use support::Project;

#[test]
fn popen_reaps_live_and_discarded_streams_with_a_session_finalizer() {
    let Some(php) = oracle::find_php() else {
        return;
    };
    for program in [php.as_os_str(), env!("CARGO_BIN_EXE_phera").as_ref()] {
        let project = Project::new();
        let script = project.write(
            "shutdown.php",
            r#"<?php
            function run_child($name) {
                $path = escapeshellarg(__DIR__ . '/' . $name);
                $after = $name === 'held'
                    ? '; cat ' . escapeshellarg(__DIR__ . '/sess_popen-finalizer')
                    : '';
                $pipe = popen("(cat; sleep 0.1; printf ':done'$after) > $path 2>/dev/null", 'w');
                fwrite($pipe, $name);
                return $pipe;
            }
            $held = run_child('held');
            $discarded = run_child('discarded');
            unset($discarded);

            session_save_path(__DIR__);
            session_id('popen-finalizer');
            session_start();
            $_SESSION['state'] = 'saved';
            echo 'script done';
            "#,
        );
        let mut command = Command::new(program);
        if program == php.as_os_str() {
            command.arg("-n");
        }
        let output = command
            .args([
                "-d",
                "session.use_cookies=0",
                "-d",
                "session.cache_limiter=",
            ])
            .arg(&script)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"script done");
        assert!(output.stderr.is_empty(), "{output:?}");
        for name in ["held", "discarded"] {
            assert_eq!(
                fs::read_to_string(project.0.join(name)).unwrap(),
                if name == "held" {
                    r#"held:donestate|s:5:"saved";"#
                } else {
                    "discarded:done"
                },
                "{program:?} returned before reaping {name}"
            );
        }
        assert_eq!(
            fs::read_to_string(project.0.join("sess_popen-finalizer")).unwrap(),
            r#"state|s:5:"saved";"#
        );
    }
}

#[test]
fn cli_process_control_stubs_explain_why_operations_are_unavailable() {
    let project = Project::new();
    let script = project.write(
        "pcntl.php",
        r#"<?php
        echo SIGINT, ':', SIGTERM, ' ', (int) function_exists('pcntl_signal'), "
";
        $status = 0;
        foreach ([
            fn () => pcntl_fork(),
            fn () => pcntl_exec('/bin/true'),
            fn () => pcntl_wait($status),
            fn () => pcntl_waitpid(1, $status),
            fn () => pcntl_waitid(),
        ] as $call) {
            try { $call(); }
            catch (Phera\NotSupportedException $error) { echo $error->getMessage(), "
"; }
        }
        "#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg(script)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("2:15 0\n"), "{text}");
    for (name, reason) in [
        (
            "pcntl_fork",
            "forking a multithreaded Tokio runtime is unsafe",
        ),
        (
            "pcntl_exec",
            "replacing the process would replace the entire Tokio runtime",
        ),
        (
            "pcntl_wait",
            "the runtime waits only for child handles it owns",
        ),
        (
            "pcntl_waitpid",
            "the runtime waits only for child handles it owns",
        ),
        (
            "pcntl_waitid",
            "the runtime waits only for child handles it owns",
        ),
    ] {
        assert!(
            text.contains(&format!("{name}() is not supported by Phera: {reason}\n")),
            "{text}"
        );
    }
}
