//! Startup precedence through the binary, including expansion against the winning layer.

mod support;

use std::path::Path;
use std::process::{Command, Output};

use support::Project;

fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phera"))
        .current_dir(root)
        .env_clear()
        .envs(env.iter().copied())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn all_file_subsets_and_app_env_sources() {
    let project = Project::new();
    project.write(
        "show.php",
        "<?php echo getenv('KEY'), '|', $_ENV['KEY'] ?? '', '|', \
        $_SERVER['KEY'] ?? '', '|', getenv('COPY');",
    );
    let layers = [".env", ".env.local", ".env.test", ".env.test.local"];
    // APP_ENV can come from neither source, either generic file, or the process.
    for source in 0..4 {
        for mask in 0..16 {
            for (i, name) in layers.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    let app = if source == i + 1 && i < 2 {
                        "APP_ENV=test\n"
                    } else {
                        ""
                    };
                    project.write(name, format!("{app}KEY={i}\nCOPY=\u{24}{{KEY}}\n"));
                } else if project.0.join(name).exists() {
                    std::fs::remove_file(project.0.join(name)).unwrap();
                }
            }
            for locked in [false, true] {
                let mut env = Vec::new();
                if source == 3 {
                    env.push(("APP_ENV", "test"));
                }
                if locked {
                    env.push(("KEY", "real"));
                }
                let variant =
                    source == 3 || source == 1 && mask & 1 != 0 || source == 2 && mask & 2 != 0;
                let last = (0..if variant { 4 } else { 2 })
                    .rev()
                    .find(|i| mask & (1 << i) != 0);
                let file_value = last.map(|i| i.to_string()).unwrap_or_default();
                let key = if locked { "real" } else { &file_value };
                let copy = if last.is_some() { key } else { "" };
                let output = run(&project.0, &["show.php"], &env);
                assert!(output.status.success());
                assert_eq!(
                    String::from_utf8(output.stdout).unwrap(),
                    format!("{key}|{key}|{key}|{copy}"),
                    "{mask} / {source}"
                );
            }
        }
    }
}

#[test]
fn options_script_directory_and_safe_diagnostics() {
    let project = Project::new();
    std::fs::create_dir(project.0.join("src")).unwrap();
    project.write(".env", "KEY=wrong-directory");
    project.write("src/.env", "KEY=automatic\nAPP_ENV=wrong");
    project.write("src/.env.local", "APP_ENV=test");
    project.write("src/.env.test", "KEY=selected");
    project.write("src/.env.prod", "KEY=production");
    project.write("first", "KEY=one\nFROM=explicit");
    project.write("second", "KEY=two");
    project.write(
        "src/app.php",
        "<?php echo getenv('KEY'), '|', getenv('FROM'), '|', $argc;",
    );
    for (flags, expected) in [
        (vec!["run", "src/app.php", "--no-env-file"], "selected||2"),
        (vec!["--no-env-file", "src/app.php"], "||1"),
        (
            vec![
                "--env-file",
                "first",
                "run",
                "--env-file",
                "second",
                "src/app.php",
            ],
            "two|explicit|1",
        ),
    ] {
        assert_eq!(run(&project.0, &flags, &[]).stdout, expected.as_bytes());
    }
    assert_eq!(
        run(&project.0, &["src/app.php"], &[("APP_ENV", "prod")]).stdout,
        b"production||1"
    );
    for flags in [
        vec!["--env-file", "missing", "src/app.php"],
        vec!["--env-file"],
        vec!["--env-file", "first", "--no-env-file", "src/app.php"],
    ] {
        assert!(!run(&project.0, &flags, &[]).status.success());
    }
    project.write("src/.env.local", "APP_ENV=private-profile");
    project.write(
        "src/.env.private-profile",
        b"# line\r\nTOKEN=\"private-value\"x",
    );
    let output = run(&project.0, &["src/app.php"], &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let message = String::from_utf8(output.stderr).unwrap();
    assert!(message.contains("src/.env.<APP_ENV>:2:22: TOKEN: expected"));
    assert!(!message.contains("private"));
}
