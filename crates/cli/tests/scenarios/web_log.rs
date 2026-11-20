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
