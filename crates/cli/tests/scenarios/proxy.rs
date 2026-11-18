//! `phera serve` behind a proxy: forwarding headers from a trusted address,
//! the health path, and the platform's `PORT`.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use super::super::Served;

/// Prints what a script uses to learn who is calling and how.
const WHO: &str = "<?php echo $_SERVER['REMOTE_ADDR'], '|', $_SERVER['HTTPS'] ?? 'off', '|', \
    $_SERVER['SERVER_PORT'] === (string) $_GET['real'] ? 'real-port' : $_SERVER['SERVER_PORT'], \
    '|', $_SERVER['HTTP_HOST'] === $_GET['host'] ? 'real-host' : $_SERVER['HTTP_HOST'], '|', \
    $_SERVER['SERVER_NAME'] === explode(':', $_GET['host'])[0] ? 'real-name' : $_SERVER['SERVER_NAME'], \
    '|', $_SERVER['HTTP_X_FORWARDED_FOR'] ?? '-';";

const FORWARDED: &str = "X-Forwarded-For: 203.0.113.7, 127.0.0.1\r\n\
    X-Forwarded-Proto: https\r\nX-Forwarded-Host: shop.example\r\n";

fn who(served: &Served, headers: &str) -> String {
    let port = served.address.rsplit(':').next().unwrap();
    let target = format!("/who.php?real={port}&host={}", served.address);
    let (status, body) = served.request("GET", &target, headers, "");
    assert_eq!(status, 200, "{body}");
    body
}

#[test]
fn forwarding_headers_from_a_trusted_proxy_describe_the_client() {
    // The test connects from 127.0.0.1, which plays the proxy.
    let served = Served::start_in(
        "proxy-trusted",
        1,
        &[("who.php", WHO)],
        ".",
        &["--trusted-proxies", "127.0.0.1"],
    );
    assert_eq!(
        who(&served, FORWARDED),
        "203.0.113.7|on|443|shop.example|shop.example|203.0.113.7, 127.0.0.1"
    );
    // The proxy names plain HTTP and a port.
    assert_eq!(
        who(
            &served,
            "X-Forwarded-Proto: http\r\nX-Forwarded-Port: 8080\r\n"
        ),
        "127.0.0.1|off|8080|real-host|real-name|-"
    );
    // The standard header.
    assert_eq!(
        who(
            &served,
            "Forwarded: for=198.51.100.4;proto=https;host=api.example\r\n"
        ),
        "198.51.100.4|on|443|api.example|api.example|-"
    );
    // Nothing forwarded: the request is described as it arrived.
    assert_eq!(
        who(&served, ""),
        "127.0.0.1|off|real-port|real-host|real-name|-"
    );
}

/// Without the option, anyone could claim to be any address.
#[test]
fn forwarding_headers_are_ignored_unless_the_sender_is_trusted() {
    let unconfigured = Served::start("proxy-none", &[("who.php", WHO)]);
    assert_eq!(
        who(&unconfigured, FORWARDED),
        "127.0.0.1|off|real-port|real-host|real-name|203.0.113.7, 127.0.0.1"
    );
    let other_proxy = Served::start_in(
        "proxy-other",
        1,
        &[("who.php", WHO)],
        ".",
        &["--trusted-proxies", "10.0.0.0/8"],
    );
    assert_eq!(
        who(&other_proxy, FORWARDED),
        "127.0.0.1|off|real-port|real-host|real-name|203.0.113.7, 127.0.0.1"
    );
}

#[test]
fn the_health_path_is_answered_without_php() {
    // The front controller would answer every path, and fails if it runs.
    let served = Served::start_in(
        "health",
        1,
        &[(
            "index.php",
            "<?php http_response_code(500); echo 'php ran';",
        )],
        ".",
        &["--health", "/healthz"],
    );
    assert_eq!(served.get("/healthz"), (200, "ok\n".to_string()));
    assert_eq!(served.get("/healthz?probe=1"), (200, "ok\n".to_string()));
    assert_eq!(served.get("/healthz/more"), (500, "php ran".to_string()));
    assert_eq!(served.get("/"), (500, "php ran".to_string()));
}

/// A hosting platform sets `PORT` and connects from outside the container.
#[test]
fn the_platform_port_is_listened_on_from_every_interface() {
    let root = std::env::temp_dir().join(format!("phera-serve-port-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.php"), "<?php echo 'up';").unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .unwrap()
        .port();
    let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
        .args(["serve", "--no-env-file", "--workers", "1"])
        .arg(&root)
        .env("PORT", port.to_string())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut banner = String::new();
    BufReader::new(process.stdout.take().unwrap())
        .read_line(&mut banner)
        .unwrap();
    let _ = process.kill();
    let _ = process.wait();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        banner.contains(&format!("http://0.0.0.0:{port} ")),
        "{banner}"
    );
}
