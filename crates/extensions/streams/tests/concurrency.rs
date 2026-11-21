//! A FIFO read on one server worker leaves that worker free for other requests.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use phera_extension_api::testing::serve::Server;

#[test]
fn fifo_waits_without_blocking_the_worker_and_keeps_state_request_local() {
    let Some(binary) = std::env::var_os("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN to exercise streams through phera serve");
        return;
    };
    let temporary = tempfile::tempdir().expect("create server fixture");
    let root = temporary
        .path()
        .canonicalize()
        .expect("canonical server root");
    fs::write(root.join("small.txt"), b"small").expect("write small file");
    fs::create_dir(root.join("private")).expect("create private cwd");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(root.join("fifo"))
            .status()
            .expect("create FIFO")
            .success()
    );
    fs::write(
        root.join("wait.php"),
        r#"<?php
        $pipe = fopen('fifo', 'r');
        chdir('private');
        umask(0077);
        file_put_contents('ready', 'ready');
        $tmp = tmpfile();
        $leak = [];
        $leak['self'] =& $leak;
        $leak['stream'] = $tmp;
        file_put_contents('temporary-path', stream_get_meta_data($tmp)['uri']);
        echo file_get_contents('ready'), ':', fread($pipe, 1), ':', fileperms('ready') & 0777;
    "#,
    )
    .expect("write waiting PHP route");
    fs::write(
        root.join("quick.php"),
        r#"<?php
        echo basename(getcwd()), ':', decoct(umask()), ':';
        echo file_exists('small.txt') ? file_get_contents('small.txt') : 'missing';
    "#,
    )
    .expect("write quick PHP route");
    fs::write(
        root.join("input.php"),
        r#"<?php
        $first = fopen('php://input', 'r');
        echo bin2hex(fread($first, 2)), ':';
        echo bin2hex(file_get_contents('php://input')), ':';
        echo bin2hex(stream_get_contents($first));
    "#,
    )
    .expect("write body-reading route");
    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("fifo"))
        .expect("open FIFO for the delayed byte");
    let server = Server::start(&PathBuf::from(binary), &root, 1, &[]);
    let start = Instant::now();
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| server.get_one("/wait.php"));
        let unblock = scope.spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            pipe.write_all(b"x").expect("unblock FIFO");
        });
        let marker = root.join("private/temporary-path");
        while !marker.is_file() && start.elapsed() < Duration::from_millis(800) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let temporary_path =
            fs::read_to_string(marker).expect("waiting request opened its temporary file");
        assert!(std::path::Path::new(&temporary_path).is_file());
        let responses = server.get_all((0..20).map(|request| format!("/quick.php?n={request}")));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "quick requests waited for the FIFO"
        );
        let name = root.file_name().expect("fixture name").to_string_lossy();
        for response in responses {
            assert!(
                response.ends_with(&format!("{name}:22:small")),
                "{response}"
            );
        }
        unblock.join().expect("FIFO writer");
        let response = waiting.join().expect("waiting request");
        assert!(response.ends_with("ready:x:384"), "{response}");
        assert!(!std::path::Path::new(&temporary_path).exists());
    });
    let body = server.post_one("/input.php", &[0, 255, b'c', b'd']);
    assert!(body.ends_with("00ff:00ff6364:6364"), "{body}");
    let empty = server.get_one("/input.php");
    assert!(empty.ends_with("::"), "{empty}");
}
