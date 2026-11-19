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
        &["--dev", "-d", "display_errors=stderr", "-d", "log_errors=0"],
    );
    let (status, body) = dev.get("/throw.php");
    assert_eq!(status, 200);
    assert!(body.starts_with(
        "<br />\n<b>Fatal error</b>:  Uncaught RuntimeException: uncaught &lt; &amp; &quot; &gt;"
    ));
    assert!(body.contains("#1 {main}"));
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
                "a".repeat(71)
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
