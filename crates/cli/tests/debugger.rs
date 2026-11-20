//! Exercise the CLI's actual debugger connection. The fixture comes from Xdebug 3.5.0.

mod debugger {
    pub(super) mod peer;
    pub(super) mod snapshot;
    pub(super) mod xml;
}

use std::path::{Path, PathBuf};
use std::process::Command;

use debugger::peer::Peer;
use debugger::xml::Normalize;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/debugger");

fn phera_executable() -> PathBuf {
    std::env::var_os("PHERA_DEBUGGER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_phera")))
}

fn phera(port: u16, script: &Path) -> Command {
    let executable = phera_executable();
    let mut command = Command::new(executable);
    command.args([
        "run",
        "--debug",
        "-d",
        &format!("xdebug.client_port={port}"),
    ]);
    command.arg(script);
    command
}

fn xdebug(port: u16, script: &Path, ini: &Path) -> Command {
    let php = phera_extension_api::testing::oracle::find_php().expect("PHP oracle");
    let mut command = Command::new(php);
    command.arg("-c").arg(ini).args([
        "-d",
        "xdebug.mode=debug",
        "-d",
        "xdebug.start_with_request=yes",
        "-d",
        &format!("xdebug.client_port={port}"),
    ]);
    command.arg(script);
    command
}

fn commands(script: &Path, scenario: &str) -> String {
    let uri = url::Url::from_file_path(script).expect("absolute source path");
    scenario.replace("{file}", uri.as_str())
}

fn session(scenario: &str, name: &str, stdout: &[u8]) -> Vec<String> {
    let script = Path::new(FIXTURES).join(format!("{name}.php"));
    let commands = commands(&script, scenario);
    let mut normalize = Normalize::default();
    let (packets, output) = if let Some(ini) = std::env::var_os("PHERA_RECORD_DEBUG_ORACLE") {
        let ini = PathBuf::from(ini);
        Peer::launch(|port| xdebug(port, &script, &ini))
            .expect("Xdebug peer")
            .collect(&commands)
            .expect("Xdebug session")
    } else {
        Peer::launch(|port| phera(port, &script))
            .expect("Phera peer")
            .collect(&commands)
            .expect("Phera session")
    };
    assert!(output.status.success(), "{:?}", output.status);
    assert_eq!(output.stdout, stdout);
    let normalized: Vec<_> = packets
        .iter()
        .map(|xml| normalize.packet(xml, &script.to_string_lossy()))
        .collect();
    if std::env::var_os("PHERA_RECORD_DEBUG_ORACLE").is_some() {
        let path = Path::new(FIXTURES).join(format!("{name}.xdebug.xml"));
        std::fs::write(path, normalized.join("\n") + "\n").expect("write Xdebug fixture");
    }
    normalized
}

#[test]
fn line_breakpoint_inspection_steps_and_eval() {
    let actual = session(include_str!("debugger/session.commands"), "session", b"2\n");
    if std::env::var_os("PHERA_RECORD_DEBUG_ORACLE").is_some() {
        return;
    }
    compare(&actual, include_str!("debugger/session.xdebug.xml"));
}

#[test]
fn condition_generator_native_values_and_exception() {
    let actual = session(include_str!("debugger/flow.commands"), "flow", b"27\n");
    if std::env::var_os("PHERA_RECORD_DEBUG_ORACLE").is_some() {
        return;
    }
    compare(&actual, include_str!("debugger/flow.xdebug.xml"));
}

#[test]
fn streams_and_programmatic_break() {
    if std::env::var_os("PHERA_RECORD_DEBUG_ORACLE").is_some() {
        return;
    }
    let script = Path::new(FIXTURES).join("streams.php");
    let commands = "stdout -c 1\nstderr -c 2\nrun\ndetach\n";
    let (packets, output) = Peer::launch(|port| phera(port, &script))
        .expect("Phera peer")
        .collect(commands)
        .expect("redirected Phera streams");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"copied\nfinished\n");
    assert!(output.stderr.is_empty());
    assert!(packets.iter().any(|packet| {
        packet.contains(r#"<stream"#)
            && packet.contains(r#"type="stdout""#)
            && packet.contains("Y29waWVkCg==")
    }));
    assert!(packets.iter().any(|packet| {
        packet.contains(r#"<stream"#)
            && packet.contains(r#"type="stderr""#)
            && packet.contains("cmVkaXJlY3RlZAo=")
    }));
    assert!(packets.iter().any(|packet| {
        packet.contains(r#"command="run""#) && packet.contains(r#"status="break""#)
    }));
}

fn compare(actual: &[String], expected: &str) {
    assert!(
        actual
            .first()
            .is_some_and(|packet| packet.contains(">Phera</engine>"))
    );
    let expected: Vec<_> = expected.lines().collect();
    assert_eq!(actual.len(), expected.len(), "session packet count");
    let differences: Vec<_> = actual
        .iter()
        .zip(expected)
        .enumerate()
        .skip(1)
        .filter(|(_, (packet, expected))| packet.as_str() != *expected)
        .map(|(index, (packet, expected))| {
            format!("Packet {index}\nXdebug: {expected}\nPhera:  {packet}")
        })
        .collect();
    assert!(
        differences.is_empty(),
        "{} packet differences\n{}",
        differences.len(),
        differences.join("\n")
    );
}
