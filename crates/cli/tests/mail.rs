//! Compare Unix mail delivery with a sendmail stub that records arguments and stdin.

mod support;

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use phera_extension_api::testing::oracle;
use support::Project;

#[test]
fn sends_the_same_bytes_and_arguments_as_php() {
    let Some(php) = oracle::find_php() else {
        return;
    };
    let project = Project::new();
    let fake = project.write(
        "sendmail",
        r#"#!/bin/sh
status=$1
shift
printf '<%s>\n' "$@" >> arguments
cat >> messages
exit "$status"
"#,
    );
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let script = project.write(
        "send.php",
        r#"<?php
var_dump(mail(
    "a@example.test\r\nBcc: injected \t",
    "one\r\n\tcontinued\r\nTo: injected",
    "first\r\nmessage",
    ['From' => 'sender@example.test', 'X-List' => ['one', "two\r\n continued"]],
    "-f sender@example.test;literal"
));
var_dump(mail('to', 'string headers', 'message', "X-First: hello\nX-Second: bye \r\n"));
var_dump(mb_send_mail('to', 'Café', 'message', ['X-Field' => 'value'], null));
var_dump(mb_send_mail('to', "a\r\nBcc: value", 'message'));
var_dump(mb_send_mail('to', 'subject', 'message', ['Content-Transfer-Encoding' => '7bit']));
"#,
    );
    for (status, settings) in [
        (
            0,
            &["mail.add_x_header=1", "sendmail_from=ignored@example.test"][..],
        ),
        (
            1,
            &[
                "mail.force_extra_parameters=-f forced@example.test",
                "mail.cr_lf_mode=lf",
            ][..],
        ),
        (75, &["mail.force_extra_parameters="][..]),
    ] {
        let expected = delivery(&php, &project, &fake, &script, status, settings, true);
        let actual = delivery(
            Path::new(env!("CARGO_BIN_EXE_phera")),
            &project,
            &fake,
            &script,
            status,
            settings,
            false,
        );
        assert_eq!(actual, expected, "sendmail exit {status}");
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Delivery {
    stdout: String,
    messages: String,
    arguments: String,
    log: Vec<String>,
}

fn delivery(
    binary: &Path,
    project: &Project,
    sendmail: &Path,
    script: &Path,
    status: i32,
    settings: &[&str],
    php: bool,
) -> Delivery {
    let mut command = Command::new(binary);
    command.current_dir(&project.0).env_clear();
    for name in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if php {
        command.arg("-n");
    }
    let path = sendmail.to_string_lossy().replace('\'', "'\\''");
    command.args(["-d", "display_errors=1", "-d", "date.timezone=UTC"]);
    command.args(["-d", &format!("sendmail_path='{path}' {status}")]);
    command.args(["-d", "mail.log=mail.log"]);
    for setting in settings {
        command.args(["-d", setting]);
    }
    let output = command.arg(script).output().unwrap();
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{output:?}"
    );
    let read_and_remove = |name| {
        let path = project.0.join(name);
        let contents = fs::read_to_string(&path).unwrap();
        fs::remove_file(path).unwrap();
        contents
    };
    Delivery {
        stdout: String::from_utf8(output.stdout).unwrap(),
        messages: read_and_remove("messages"),
        arguments: read_and_remove("arguments"),
        log: read_and_remove("mail.log")
            .lines()
            .map(|line| {
                let (timestamp, message) = line.split_once("] ").unwrap();
                assert!(timestamp.ends_with(" UTC"));
                message.to_owned()
            })
            .collect(),
    }
}
