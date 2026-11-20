//! An unread stderr pipe must not turn PHP logging into a request timeout.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Stdio;
use std::time::Duration;

use crate::Served;

#[test]
fn an_unread_stderr_pipe_does_not_stall_php_requests() {
    let site = Served::start_with_stderr(
        "unread-web-log",
        1,
        &[
            (
                "flood.php",
                r#"<?php
$message = str_repeat('x', 32 * 1024);
for ($i = 0; $i < 80; ++$i) {
    if (!error_log($message, $i % 2 ? 4 : 0)) {
        exit('log failed');
    }
}
echo 'logged';
"#,
            ),
            (
                "warning.php",
                "<?php trigger_error('still serving', E_USER_WARNING); echo 'continued';",
            ),
        ],
        ".",
        &[
            "--request-timeout",
            "2",
            "-d",
            "display_errors=0",
            "-d",
            "log_errors=1",
        ],
        Stdio::piped(),
    );
    // Keep the read end open and unread for both requests. The second
    // request also exercises diagnostics while the log writer is blocked.
    assert!(site.process.stderr.is_some());
    for (route, body) in [("/flood.php", "logged"), ("/warning.php", "continued")] {
        let mut stream = TcpStream::connect(&site.address).expect("connect to PHP server");
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("set response deadline");
        write!(
            stream,
            "GET {route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .expect("send HTTP request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        let (headers, response_body) = response.split_once("\r\n\r\n").expect("HTTP response");
        assert!(headers.starts_with("HTTP/1.1 200 "), "{response}");
        assert_eq!(response_body, body);
    }
}

/// Records that did not fit while nobody read the log are counted, and the
/// count is written once the log is read again.
#[test]
fn dropped_log_records_are_reported_when_the_log_is_read_again() {
    let mut site = Served::start_with_stderr(
        "dropped-web-log",
        1,
        &[(
            "flood.php",
            // 8 MiB of log text: twice what may wait for the writer.
            r#"<?php
$message = str_repeat('x', 32 * 1024);
for ($i = 0; $i < 256; ++$i) {
    error_log($message);
}
echo 'logged';
"#,
        )],
        ".",
        &["--request-timeout", "5", "-d", "log_errors=1"],
        Stdio::piped(),
    );
    let mut stream = TcpStream::connect(&site.address).expect("connect to PHP server");
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .expect("set response deadline");
    write!(
        stream,
        "GET /flood.php HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send HTTP request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    assert!(response.ends_with("logged"), "{response}");

    // Now read the log: the queued records come first, then the count.
    let mut stderr = site.process.stderr.take().expect("stderr pipe");
    let (found, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        const NOTICE: &[u8] = b"log records were dropped";
        let mut log = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        while let Ok(read) = stderr.read(&mut chunk) {
            if read == 0 {
                break;
            }
            // The notice may straddle two reads: look again from a little
            // before the new bytes.
            let from = log.len().saturating_sub(NOTICE.len());
            log.extend_from_slice(&chunk[..read]);
            let Some(at) = log[from..]
                .windows(NOTICE.len())
                .position(|window| window == NOTICE)
            else {
                continue;
            };
            let start = log[..from + at]
                .iter()
                .rposition(|&byte| byte == b'\n')
                .map_or(0, |newline| newline + 1);
            let line = String::from_utf8_lossy(&log[start..from + at + NOTICE.len()]);
            let _ = found.send(line.into_owned());
            break;
        }
    });
    let line = done
        .recv_timeout(Duration::from_secs(8))
        .expect("the server reports dropped log records");
    assert!(line.starts_with("phera-server: "), "{line}");
}
