//! Diagnostic destinations for `phera serve` and CLI scripts.

use std::process::Command;

use phera_extension_api::testing::oracle;

use super::super::Served;

const WARNINGS: &str = r#"<?php
trigger_error('warn < & " >', E_USER_WARNING);
trigger_error('notice < & " >', E_USER_NOTICE);
trigger_error('deprecated < & " >', E_USER_DEPRECATED);
echo 'done';
"#;

const THROW: &str = r#"<?php
function fail($message) { throw new RuntimeException($message); }
fail('uncaught < & " >');
"#;

const FATAL: &str = r#"<?php
function fail($message) { trigger_error($message, E_USER_ERROR); }
fail('fatal < & " >');
"#;

#[test]
fn cli_display_destinations_and_format_match_php() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let site = Served::start(
        "diagnostic-oracle",
        &[
            ("warn.php", WARNINGS),
            ("throw.php", THROW),
            ("parse.php", "<?php echo ;"),
        ],
    );
    for file in ["warn.php", "throw.php", "parse.php"] {
        let script = site.root.join(file);
        for display in ["0", "1", "stderr", "stdout"] {
            for html in ["0", "1"] {
                for mask in ["0", "-1"] {
                    let settings = [
                        "-d",
                        &format!("display_errors={display}"),
                        "-d",
                        &format!("html_errors={html}"),
                        "-d",
                        &format!("error_reporting={mask}"),
                    ];
                    let expected = Command::new(&php)
                        .arg("-n")
                        .args(settings)
                        .arg(&script)
                        .output()
                        .expect("run PHP");
                    let actual = Command::new(env!("CARGO_BIN_EXE_phera"))
                        .args(settings)
                        .arg(&script)
                        .output()
                        .expect("run Phera");
                    assert_eq!(
                        actual.status.code(),
                        expected.status.code(),
                        "{file} {settings:?}"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&actual.stdout),
                        String::from_utf8_lossy(&expected.stdout),
                        "{file} {settings:?}"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&actual.stderr),
                        String::from_utf8_lossy(&expected.stderr),
                        "{file} {settings:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn optional_fatal_trace_follows_cli_diagnostic_destinations() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let site = Served::start("fatal-trace-oracle", &[("fatal.php", FATAL)]);
    let script = site.root.join("fatal.php");
    for trace in ["0", "1"] {
        for display in ["0", "1", "stderr", "stdout"] {
            for html in ["0", "1"] {
                for log in ["0", "1"] {
                    let settings = [
                        "-d",
                        &format!("fatal_error_backtraces={trace}"),
                        "-d",
                        &format!("display_errors={display}"),
                        "-d",
                        &format!("html_errors={html}"),
                        "-d",
                        &format!("log_errors={log}"),
                    ];
                    let expected = Command::new(&php)
                        .arg("-n")
                        .args(settings)
                        .arg(&script)
                        .output()
                        .expect("run PHP");
                    let actual = Command::new(env!("CARGO_BIN_EXE_phera"))
                        .args(settings)
                        .arg(&script)
                        .output()
                        .expect("run Phera");
                    assert_eq!(actual.status.code(), expected.status.code(), "{settings:?}");
                    assert_eq!(
                        String::from_utf8_lossy(&actual.stdout),
                        String::from_utf8_lossy(&expected.stdout),
                        "{settings:?}"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&actual.stderr),
                        String::from_utf8_lossy(&expected.stderr),
                        "{settings:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn server_logs_optional_fatal_trace_with_display_disabled() {
    let site = Served::start_in(
        "fatal-trace-server",
        1,
        &[("fatal.php", FATAL)],
        ".",
        &[
            "--dev",
            "-d",
            "display_errors=0",
            "-d",
            "fatal_error_backtraces=1",
            "-d",
            "error_log=php.log",
        ],
    );
    assert_eq!(site.get("/fatal.php"), (500, String::new()));
    let log = std::fs::read_to_string(site.root.join("php.log")).expect("read fatal error log");
    assert!(log.contains("PHP Fatal error:  fatal < & \" > in "));
    assert!(log.contains("Stack trace:\n#0 "));
    assert!(log.contains("trigger_error("));
    assert!(log.contains("#2 {main}"));
}

#[test]
fn server_hides_diagnostics_by_default_and_dev_can_display_them() {
    let site = Served::start(
        "diagnostic-server",
        &[
            ("warn.php", WARNINGS),
            (
                "fiber.php",
                "<?php (new Fiber(fn() => trigger_error('inside fiber', E_USER_WARNING)))->start(); echo 'done';",
            ),
            ("throw.php", THROW),
            ("parse.php", "<?php echo ;"),
            ("included.php", "<?php require __DIR__ . '/parse.php';"),
            (
                "logged.php",
                "<?php ini_set('error_log', __DIR__ . '/php.log'); throw new RuntimeException('to file');",
            ),
        ],
    );
    assert_eq!(site.get("/warn.php"), (200, "done".to_string()));
    assert_eq!(site.get("/fiber.php"), (200, "done".to_string()));
    for script in ["/throw.php", "/parse.php", "/included.php", "/logged.php"] {
        assert_eq!(site.get(script), (500, String::new()), "{script}");
    }
    let log =
        std::fs::read_to_string(site.root.join("php.log")).expect("append diagnostic to error_log");
    assert!(log.contains("PHP Fatal error:  Uncaught RuntimeException: to file"));
    assert!(log.contains("Stack trace:\n#0 {main}\n"));

    let dev = Served::start_in(
        "diagnostic-dev",
        1,
        &[("throw.php", THROW)],
        ".",
        &["--dev", "-d", "log_errors=0"],
    );
    let (status, body) = dev.get("/throw.php");
    assert_eq!(status, 200);
    assert!(body.starts_with(
        "<br />\n<b>Fatal error</b>:  Uncaught RuntimeException: uncaught &lt; &amp; &quot; &gt;"
    ));
    assert!(body.contains("#1 {main}"));
}

#[test]
fn entry_compile_diagnostics_use_the_response_or_log() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    for flags in [
        &["-d", "error_log=php.log"][..],
        &["--dev", "-d", "html_errors=0", "-d", "log_errors=0"][..],
    ] {
        let site = Served::start_in(
            "entry-compile-diagnostic",
            1,
            &[("index.php", "<?php echo ;")],
            ".",
            flags,
        );
        let (status, body) = site.get("/fallback");
        if flags.contains(&"--dev") {
            let expected = Command::new(&php)
                .args(["-n", "-d", "display_errors=1", "-d", "log_errors=0"])
                .arg(site.root.join("index.php").canonicalize().unwrap())
                .output()
                .expect("run PHP");
            assert_eq!(expected.status.code(), Some(255));
            assert_eq!(status, 200);
            assert_eq!(body.as_bytes(), expected.stdout);
        } else {
            assert_eq!((status, body), (500, String::new()));
            let log = std::fs::read_to_string(site.root.join("php.log")).unwrap();
            assert!(log.contains("PHP Parse error:  syntax error, unexpected token"));
            assert!(log.contains("index.php on line 1"));
        }
    }
}

#[test]
fn the_startup_display_setting_and_rfc_boundary_limit_apply_before_php() {
    for setting in ["0", "1"] {
        let flag = format!("display_startup_errors={setting}");
        let site = Served::start_in(
            &format!("diagnostic-startup-{setting}"),
            1,
            &[(
                "index.php",
                "<?php echo 'form:', $_POST['title'] ?? 'empty';",
            )],
            ".",
            &[
                "--dev",
                "-d",
                "html_errors=0",
                "-d",
                "log_errors=0",
                "-d",
                &flag,
            ],
        );
        let (status, text) = site.request(
            "POST",
            "/",
            "Content-Type: multipart/form-data; boundary=\"unterminated\r\n",
            "form",
        );
        assert_eq!(status, 200);
        if setting == "1" {
            assert_eq!(
                text,
                "\nWarning: PHP Request Startup: Invalid boundary in multipart/form-data POST data in Unknown on line 0\nform:empty"
            );
            let headers = format!(
                "Content-Type: multipart/form-data; boundary={}\r\n",
                "a".repeat(5 * 1024 - 3)
            );
            let (_, text) = site.request("POST", "/", &headers, "form");
            assert_eq!(
                text,
                "\nWarning: Boundary too large in multipart/form-data POST data in Unknown on line 0\nform:empty"
            );
        } else {
            assert_eq!(text, "form:empty");
        }
        let (_, text) = site.request(
            "POST",
            "/",
            "Content-Type: multipart/form-data; boundary=\"\"\r\n",
            "--\r\nContent-Disposition: form-data; name=title\r\n\r\nempty-boundary\r\n----\r\n",
        );
        assert_eq!(text, "form:empty-boundary");
    }
}
