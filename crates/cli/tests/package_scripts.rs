//! Exercise install scripts through the CLI and the real PHP callback loader.

use serde_json::json;
use std::fs;
use std::process::Command;

#[test]
fn install_scripts_share_environment_and_pass_event_arguments() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    let manifest = json!({
        "name": "test/package-scripts",
        "config": {"vendor-dir": "dependencies"},
        "autoload": {"classmap": ["Callbacks.php"]},
        "scripts": {
            "post-autoload-dump": [
                "@putenv PHERA_SCRIPT_VALUE=fixture",
                "@callback \"two words\"",
                "@php -r 'file_put_contents(\"inline.txt\", getenv(\"PHERA_SCRIPT_VALUE\"));'",
                "@putenv PHERA_SCRIPT_VALUE",
                "@php -r 'file_put_contents(\"unset.txt\", getenv(\"PHERA_SCRIPT_VALUE\") === false ? \"unset\" : \"set\");'",
                "@composer dump-autoload --no-scripts",
            ],
            "callback": ["Callbacks::postDump"]
        }
    });
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    fs::write(
        root.join("composer.lock"),
        r#"{"content-hash":"fixture","packages":[],"packages-dev":[]}"#,
    )
    .unwrap();
    fs::write(
        root.join("Callbacks.php"),
        r#"<?php
        class Callbacks {
            public static function postDump(Composer\Script\Event $event) {
                $vendor = $event->getComposer()->getConfig()->get('vendor-dir');
                file_put_contents('event.json', json_encode([
                    $event->getName(), $event->isDevMode(), $event->getArguments(),
                    basename($vendor), getenv('PHERA_SCRIPT_VALUE')
                ]));
            }
        }
    "#,
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_phera"));
    command.env_clear();
    for key in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let output = command
        .current_dir(root)
        .env("PHERA_CACHE_DIR", root.join("cache"))
        .args(["install", "--no-audit", "--no-interaction"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let event: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("event.json")).unwrap()).unwrap();
    assert_eq!(
        event,
        json!(["callback", true, ["two words"], "dependencies", "fixture"])
    );
    assert_eq!(fs::read(root.join("inline.txt")).unwrap(), b"fixture");
    assert_eq!(fs::read(root.join("unset.txt")).unwrap(), b"unset");
}
