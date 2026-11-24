//! A request debugger can stop inside a file that the worker included during boot.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::time::Duration;

use super::peer::Peer;

#[test]
fn a_fork_resolves_and_hits_a_breakpoint_in_preloaded_code() {
    if std::env::var_os("PHERA_RECORD_DEBUG_ORACLE").is_some() {
        return;
    }
    let directory = tempfile::tempdir().expect("temporary site");
    let root = directory.path().canonicalize().expect("absolute site path");
    let file = root.join("index.php");
    let included = root.join("boot-include.php");
    std::fs::write(
        &file,
        "<?php\nrequire __DIR__ . '/boot-include.php';\n$boot = 42;\nPhera\\snapshot();\necho increment($boot);\n",
    )
    .expect("entry script");
    std::fs::write(
        &included,
        "<?php\nfunction increment(int $boot): int {\n    return $boot + 1;\n}\n",
    )
    .expect("boot include");
    let site = TcpListener::bind("127.0.0.1:0").expect("local HTTP address");
    let address = site.local_addr().expect("bound HTTP address");
    drop(site);

    let mut ide = Peer::launch(|port| {
        let mut server = Command::new(crate::phera_executable());
        server.current_dir(&root);
        server.args([
            "serve",
            "--debug",
            "--dev",
            "--debug-boot",
            "--workers",
            "1",
            "--listen",
            &address.to_string(),
            "-d",
            &format!("xdebug.client_port={port}"),
        ]);
        server.arg(&file);
        server
    })
    .expect("boot connection");
    assert!(ide.read().expect("boot init").contains("<init"));
    assert!(exchange(&mut ide, "run", 1).contains(r#"status="stopping""#));
    exchange(&mut ide, "stop", 2);

    let file = url::Url::from_file_path(&file).expect("boot file URI");
    let included = url::Url::from_file_path(&included).expect("included file URI");
    for _ in 0..2 {
        let request = std::thread::spawn(move || get(address));
        ide.reconnect()
            .expect("a fresh debugger session for this request");
        assert!(ide.read().expect("request init").contains("<init"));
        exchange(&mut ide, "feature_set -n resolved_breakpoints -v 1", 1);
        exchange(
            &mut ide,
            &format!("breakpoint_set -t line -f {file} -n 3"),
            2,
        );
        let resolution = exchange(
            &mut ide,
            &format!("breakpoint_set -t line -f {included} -n 3"),
            3,
        );
        assert!(
            resolution.contains(r#"resolved="resolved""#),
            "{resolution}"
        );
        let stop = exchange(&mut ide, "run", 4);
        assert!(
            stop.contains(included.as_str()) && stop.contains(r#"lineno="3""#),
            "{stop}"
        );
        assert!(exchange(&mut ide, "run", 5).contains(r#"status="stopping""#));
        exchange(&mut ide, "stop", 6);
        let response = request.join().expect("completed HTTP request");
        assert!(response.starts_with("HTTP/1.1 200") && response.ends_with("\r\n\r\n43"));
    }
}

fn exchange(peer: &mut Peer, command: &str, id: usize) -> String {
    peer.send(command, id).expect("send debugger command");
    let packet = peer.read().expect("read debugger response");
    assert!(
        packet.contains(&format!(r#"transaction_id="{id}""#)),
        "{packet}"
    );
    packet
}

fn get(address: SocketAddr) -> String {
    let timeout = Duration::from_secs(10);
    let mut stream = TcpStream::connect_timeout(&address, timeout).expect("connect to site");
    stream
        .set_read_timeout(Some(timeout))
        .expect("HTTP read timeout");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("send HTTP request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read HTTP response");
    response
}
