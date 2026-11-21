//! Compare loopback I/O without relying on port numbers or operating system flags.

use std::path::Path;

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn sockets_match_php_on_loopback() {
    let Some(oracle) = Comparison::from_environment("sockets") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_cases/loopback.php");
    oracle.compare_all(&[script], |_, _, _| {});
}

#[test]
fn socket_select_matches_php_for_close_reset_and_urgent_data() {
    use std::{
        io::ErrorKind,
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };

    use socket2::SockRef;

    let Some(oracle) = Comparison::from_environment("sockets-readiness") else {
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_cases/readiness.php");
    let mut servers = Vec::new();
    let result = oracle.compare(&script, |_, command| {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        command.arg(listener.local_addr().unwrap().port().to_string());
        listener.set_nonblocking(true).unwrap();
        servers.push(thread::spawn(move || {
            for action in ["close", "reset", "urgent"] {
                let deadline = Instant::now() + Duration::from_secs(5);
                let peer = loop {
                    match listener.accept() {
                        Ok((peer, _)) => break peer,
                        Err(error)
                            if error.kind() == ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept for {action}: {error}"),
                    }
                };
                peer.set_nonblocking(false).unwrap();
                thread::sleep(Duration::from_millis(30));
                let socket = SockRef::from(&peer);
                if action == "reset" {
                    socket.set_linger(Some(Duration::ZERO)).unwrap();
                }
                if action == "urgent" {
                    socket.send_out_of_band(b"!").unwrap();
                    // Keep the connection open until the client has observed priority readiness.
                    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut ack = [0];
                    use std::io::Read;
                    (&peer).read_exact(&mut ack).unwrap();
                }
            }
        }));
    });
    for server in servers {
        server.join().unwrap();
    }
    let matched = result.unwrap();
    let output = String::from_utf8(matched.stdout).unwrap();
    assert!(output.contains("close wait [1,[\"socket\"]]"));
    assert!(output.contains("reset wait [1,[\"socket\"]]"));
    assert!(output.contains("urgent [1,[\"socket\"]]"));
}
