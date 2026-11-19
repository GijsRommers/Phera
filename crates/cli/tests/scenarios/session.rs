//! Cookie-bearing requests run against `php -S` and one Phera worker.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use phera_extension_api::testing::{oracle, serve::Server};

use super::super::{Served, raw_response};

const SCRIPT: &str = r#"<?php
$action = $_GET['action'];
$store = $_GET['store'];
session_save_path(__DIR__ . "/stores/$store");
ini_set('session.gc_probability', 0);
$limiter = $_GET['limiter'] ?? 'nocache';
session_cache_limiter($limiter);

if ($action === 'isolate') {
    echo json_encode([session_status(), session_id(), isset($_SESSION), session_name()]);
    return;
}
if ($action === 'cookie') {
    setcookie('other', 'keep');
    session_set_cookie_params(['lifetime' => 90, 'path' => '/account',
        'domain' => 'example.test', 'secure' => true, 'partitioned' => true,
        'httponly' => true, 'samesite' => 'Lax']);
}
if ($action === 'strict') {
    ini_set('session.use_strict_mode', 1);
}
if ($action === 'user') {
    class Wrapper extends SessionHandler {
        public function read(string $id): string|false { return parent::read($id); }
        public function write(string $id, string $data): bool { return parent::write($id, $data); }
    }
    session_set_save_handler(new Wrapper());
}
if ($action === 'invalid') {
    set_error_handler(function ($type, $message) {
        $GLOBALS['warnings'][] = explode(' (path: ', $message)[0];
        return true;
    });
    $warnings = [];
    $started = session_start();
    if ($started) {
        session_abort();
    }
    echo json_encode([$started, $warnings, glob(__DIR__ . "/stores/$store/*") !== []]);
    return;
}

$started = session_start($action === 'read' ? ['read_and_close' => true] : []);
$id = session_id();
$count = $_SESSION['count'] ?? 0;
if ($action === 'regenerate') {
    session_regenerate_id(true);
}
if ($action === 'destroy') {
    session_destroy();
} elseif ($action === 'read') {
    $_SESSION['count'] = -1;
} else {
    if ($action === 'hold' || $action === 'timeout') {
        file_put_contents(__DIR__ . "/stores/$store/locked", '1');
        usleep($action === 'timeout' ? 3000000 : 300000);
    }
    $_SESSION['count'] = ++$count;
}
$cookieCount = 0;
foreach (headers_list() as $line) {
    if (str_starts_with($line, 'Set-Cookie: PHPSESSID=')) {
        ++$cookieCount;
    }
}
echo json_encode([$started, session_status(), $count, strlen($id), $cookieCount]);
"#;

#[derive(Debug, PartialEq, Eq)]
struct Answer {
    status: u16,
    cookies: Vec<String>,
    cache: Vec<(String, String)>,
    body: String,
}

#[derive(Clone, Default)]
struct Client {
    cookie: Option<String>,
}

impl Client {
    fn request(&mut self, address: &str, path: &str) -> Answer {
        let headers = self.cookie.as_ref().map_or_else(String::new, |cookie| {
            format!("Cookie: PHPSESSID={cookie}\r\n")
        });
        let response =
            raw_response(address, "GET", path, &headers, "").expect("session HTTP response");
        let (head, body) = response.split_once("\r\n\r\n").expect("HTTP response head");
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("HTTP status");
        let mut answer = Answer {
            status,
            cookies: Vec::new(),
            cache: Vec::new(),
            body: body.into(),
        };
        for (name, value) in lines.filter_map(|line| line.split_once(':')) {
            let name = name.to_ascii_lowercase();
            let value = value.trim_start();
            match name.as_str() {
                "set-cookie" => answer.cookies.push(self.accept(value)),
                "expires" if value != "Thu, 19 Nov 1981 08:52:00 GMT" => {
                    check_date(value, 180 * 60);
                    answer.cache.push((name, "<future>".into()));
                }
                "expires" | "pragma" | "last-modified" | "cache-control" => {
                    answer.cache.push((name, value.into()));
                }
                _ => {}
            }
        }
        answer.cache.sort_unstable();
        answer
    }

    fn accept(&mut self, value: &str) -> String {
        let Some(session) = value.strip_prefix("PHPSESSID=") else {
            return value.into();
        };
        let (id, attributes) = session.split_once(';').unwrap_or((session, ""));
        assert_eq!(id.len(), 32, "{value}");
        assert!(
            id.bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        self.cookie = Some(id.into());
        let attributes = attributes
            .split(';')
            .map(|attribute| {
                if let Some(date) = attribute.strip_prefix(" expires=") {
                    check_date(date, 90);
                    " expires=<future>"
                } else {
                    attribute
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        format!("PHPSESSID=<id>;{attributes}")
    }
}

fn check_date(date: &str, seconds: u64) {
    let date = httpdate::parse_http_date(date).expect("valid HTTP date");
    let age = date
        .duration_since(std::time::SystemTime::now())
        .expect("date is in the future")
        .as_secs();
    assert!(
        (seconds - 3..=seconds).contains(&age),
        "{date:?} has age {age}"
    );
}

fn target(store: &str, action: &str, limiter: &str) -> String {
    format!("/index.php?store={store}&action={action}&limiter={limiter}")
}

fn site(name: &str, workers: usize, stores: &[&str], flags: &[&str]) -> Served {
    let server = Served::start_in(name, workers, &[("index.php", SCRIPT)], ".", flags);
    for store in stores {
        fs::create_dir_all(server.root.join("stores").join(store)).expect("create store");
    }
    server
}

#[test]
fn cookie_session_lifecycle_and_cache_headers_match_php() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = site("session-parity", 1, &["phera", "php"], &[]);
    let theirs = Server::start_php_router(&php, &ours.root, &ours.root.join("index.php"));
    let mut left = Client::default();
    let mut right = Client::default();
    for (action, limiter) in [
        ("cookie", "nocache"),
        ("count", "private"),
        ("read", "private_no_expire"),
        ("regenerate", "public"),
        ("count", ""),
        ("user", ""),
        ("destroy", ""),
    ] {
        let expected = left.request(&theirs.address, &target("php", action, limiter));
        let actual = right.request(&ours.address, &target("phera", action, limiter));
        assert_eq!(actual, expected, "{action} {limiter}");
    }
    left.cookie = Some("unknown".into());
    right.cookie = left.cookie.clone();
    assert_eq!(
        right.request(&ours.address, &target("phera", "strict", "")),
        left.request(&theirs.address, &target("php", "strict", "")),
    );
    assert_ne!(left.cookie.as_deref(), Some("unknown"));
    assert_ne!(right.cookie.as_deref(), Some("unknown"));
}

#[test]
fn two_requests_share_one_file_lock_even_across_sapis() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = site("session-lock", 2, &["shared"], &[]);
    let theirs = Server::start_php_router(&php, &ours.root, &ours.root.join("index.php"));
    let mut client = Client::default();
    let count = target("shared", "count", "");
    assert_eq!(
        client.request(&ours.address, &count).body,
        "[true,2,1,32,1]"
    );
    let marker = ours.root.join("stores/shared/locked");
    for other in [&ours.address, &theirs.address] {
        let mut left = client.clone();
        let mut right = client.clone();
        let hold = target("shared", "hold", "");
        let mut results = std::thread::scope(|scope| {
            let first = scope.spawn(|| left.request(&ours.address, &hold));
            wait_for_lock(&marker);
            let second = scope.spawn(|| right.request(other, &count));
            [first.join().unwrap().body, second.join().unwrap().body]
        });
        fs::remove_file(&marker).expect("remove lock marker");
        results.sort_unstable();
        let expected = if other == &ours.address {
            [2, 3]
        } else {
            [4, 5]
        };
        assert_eq!(
            results,
            expected.map(|count| format!("[true,2,{count},32,0]"))
        );
    }
}

#[test]
fn one_worker_isolates_sessions_and_recovers_the_lock_after_timeout() {
    let ours = site(
        "session-isolation",
        1,
        &["one"],
        &["--request-timeout", "1"],
    );
    let mut first = Client::default();
    let mut second = Client::default();
    let count = target("one", "count", "");
    assert_eq!(first.request(&ours.address, &count).body, "[true,2,1,32,1]");
    assert_eq!(
        second.request(&ours.address, &count).body,
        "[true,2,1,32,1]"
    );
    assert_ne!(first.cookie, second.cookie);
    assert_eq!(first.request(&ours.address, &count).body, "[true,2,2,32,0]");
    let id = first.cookie.as_ref().unwrap().clone();
    let file = ours.root.join("stores/one").join(format!("sess_{id}"));
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let timeout = target("one", "timeout", "");
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| first.request(&ours.address, &timeout));
        wait_for_lock(&ours.root.join("stores/one/locked"));
        let probe = target("one", "isolate", "");
        assert_eq!(
            second.request(&ours.address, &probe).body,
            "[1,\"\",false,\"PHPSESSID\"]"
        );
        assert_eq!(pending.join().unwrap().status, 504);
    });
    assert_eq!(fs::read(&file).unwrap(), b"count|i:2;");
    assert_eq!(first.request(&ours.address, &count).body, "[true,2,3,32,0]");
}

#[test]
fn malformed_cookie_ids_do_not_name_files() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = site("session-invalid-id", 1, &["phera", "php"], &[]);
    let theirs = Server::start_php_router(&php, &ours.root, &ours.root.join("index.php"));
    for id in ["a%2Fb", "a.b", "%FF", &"x".repeat(257)] {
        let mut first = Client {
            cookie: Some(id.into()),
        };
        let mut second = first.clone();
        let expected = first.request(&theirs.address, &target("php", "invalid", ""));
        let actual = second.request(&ours.address, &target("phera", "invalid", ""));
        assert_eq!(actual, expected, "{id}");
        assert_eq!(
            fs::read_dir(ours.root.join("stores/phera"))
                .unwrap()
                .count(),
            0
        );
    }
    // Keep the full decoded byte string away from the file backend, even
    // on systems where a NUL would make a C path use only its valid prefix.
    let mut nul = Client {
        cookie: Some("valid%00tail".into()),
    };
    let php_nul = nul
        .clone()
        .request(&theirs.address, &target("php", "invalid", ""));
    assert_eq!(php_nul.body, "[true,[],true]");
    assert!(ours.root.join("stores/php/sess_valid").is_file());
    let answer = nul.request(&ours.address, &target("phera", "invalid", ""));
    assert!(answer.body.starts_with("[false,"));
    assert_eq!(
        fs::read_dir(ours.root.join("stores/phera"))
            .unwrap()
            .count(),
        0
    );
}

fn wait_for_lock(marker: &std::path::Path) {
    for _ in 0..200 {
        if marker.is_file() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("request did not reach its session critical section");
}
