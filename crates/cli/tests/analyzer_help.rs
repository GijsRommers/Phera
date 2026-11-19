//! `phera lint --help` and `phera check --help` work before loading project settings.

use std::process::Command;

#[test]
fn analyzer_commands_print_their_own_help() {
    for (command, options) in [
        (
            "lint",
            &["--fix", "--only all", "--skip", "--list-rules", "--explain"][..],
        ),
        (
            "check",
            &["--level", "--generate-baseline", "--no-progress"][..],
        ),
    ] {
        let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("invalid-settings-{}-{command}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("scratch project");
        std::fs::write(directory.join("phera.toml"), "value = \"unclosed")
            .expect("write malformed configuration");
        for flag in ["--help", "-h"] {
            let output = Command::new(env!("CARGO_BIN_EXE_phera"))
                .args([command, flag])
                .current_dir(&directory)
                .output()
                .expect("run the CLI");
            assert!(output.status.success(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            let help = String::from_utf8(output.stdout).expect("UTF-8 help");
            assert!(
                help.starts_with(&format!("Usage: phera {command}")),
                "{help}"
            );
            for option in options {
                assert!(help.contains(option), "missing {option} in {help}");
            }
        }
        std::fs::remove_dir_all(directory).expect("remove scratch project");
    }
}
