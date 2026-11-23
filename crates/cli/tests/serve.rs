//! `phera serve` from the outside: start the real binary, talk HTTP to it
//! over a socket, and check what comes back.

mod scenarios {
    mod code_cache;
    mod compression;
    mod diagnostics;
    mod environment;
    mod execution_time;
    mod limits;
    mod new_project;
    mod preload;
    mod proxy;
    mod request_headers;
    mod request_parse_body;
    mod service_warmup;
    mod session;
    mod spl_policy;
    mod static_files;
    mod uploads;
    mod web_log;
}

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// A running `phera serve` and the site it serves. Stopped and cleaned up
/// when the test ends.
struct Served {
    process: Child,
    address: String,
    root: PathBuf,
    banner: String,
}

impl Served {
    fn start(name: &str, files: &[(&str, &str)]) -> Served {
        Served::start_with_workers(name, 2, files)
    }

    fn start_with_workers(name: &str, workers: usize, files: &[(&str, &str)]) -> Served {
        Self::start_in(name, workers, files, ".", &[])
    }

    /// The directory `start_in` puts the site of the test called `name` in.
    fn root_of(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("phera-serve-{name}-{}", std::process::id()))
    }

    fn start_in(
        name: &str,
        workers: usize,
        files: &[(&str, &str)],
        directory: &str,
        flags: &[&str],
    ) -> Served {
        Self::start_with_stderr(name, workers, files, directory, flags, Stdio::inherit())
    }

    fn start_with_stderr(
        name: &str,
        workers: usize,
        files: &[(&str, &str)],
        directory: &str,
        flags: &[&str],
        stderr: Stdio,
    ) -> Served {
        let root = Served::root_of(name);
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("create directory");
            std::fs::write(file, contents).expect("write file");
        }

        let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
            .args(["serve", "--listen", "127.0.0.1:0", "--workers"])
            .arg(workers.to_string())
            .current_dir(&root)
            .args(flags)
            .arg(root.join(directory))
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .expect("start phera serve");

        // The first line says where it listens: port 0 asked for a free one.
        let stdout = process.stdout.take().expect("stdout");
        let mut banner = String::new();
        BufReader::new(stdout)
            .read_line(&mut banner)
            .expect("read banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in banner: {banner:?}"))
            .to_string();
        Served {
            process,
            address,
            root,
            banner,
        }
    }

    /// Sends one request and returns the status code and the body.
    fn request(
        &self,
        method: &str,
        target: &str,
        extra_headers: &str,
        body: &str,
    ) -> (u16, String) {
        send_request(&self.address, method, target, extra_headers, body)
    }

    fn get(&self, target: &str) -> (u16, String) {
        self.request("GET", target, "", "")
    }
}

/// Sends one request to the server at `address` (ours, or real PHP's) and
/// returns the status code and the body.
fn send_request(
    address: &str,
    method: &str,
    target: &str,
    extra_headers: &str,
    body: &str,
) -> (u16, String) {
    let response = raw_response(address, method, target, extra_headers, body)
        .unwrap_or_else(|| panic!("no answer from {address} to {method} {target}"));
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status in {head:?}"));
    (status, body.to_string())
}

/// Everything the server at `address` sent back for one request, or `None`
/// when it refused, reset or closed the connection without answering.
fn raw_response(
    address: &str,
    method: &str,
    target: &str,
    extra_headers: &str,
    body: &str,
) -> Option<String> {
    let request = format!(
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Length: {}\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(address).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    (!response.is_empty()).then_some(response)
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn web_scripts_and_includes_keep_the_shebang_as_inline_text() {
    use phera_extension_api::testing::{oracle, serve::Server};

    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = Served::start(
        "shebang",
        &[
            (
                "index.php",
                "#!/usr/bin/env php\n<?php echo __LINE__, '\\n'; include __DIR__.'/include.php';",
            ),
            ("include.php", "#!include\n<?php echo __LINE__;"),
        ],
    );
    let theirs = Server::start_php(&php, &ours.root, 1);
    assert_eq!(
        ours.get("/"),
        send_request(&theirs.address, "GET", "/", "", "")
    );
}

#[test]
fn php_scripts_static_files_and_the_front_controller() {
    let served = Served::start(
        "site",
        &[
            (
                "index.php",
                "<?php echo 'front:', $_SERVER['REQUEST_URI'], ':', $_GET['q'] ?? 'none';",
            ),
            (
                "about.php",
                "<?php header('X-Page: about'); echo 'About ', 6 * 7;",
            ),
            ("css/site.css", "body { color: teal }"),
            (".env", "SECRET=1"),
            ("admin/index.php", "<?php echo 'admin area';"),
        ],
    );

    assert_eq!(served.get("/about.php"), (200, "About 42".to_string()));
    assert_eq!(
        served.get("/css/site.css"),
        (200, "body { color: teal }".to_string())
    );
    assert_eq!(served.get("/admin/"), (200, "admin area".to_string()));

    // No such file: the site's index.php answers, as frameworks expect.
    assert_eq!(
        served.get("/blog/42?q=rust"),
        (200, "front:/blog/42?q=rust:rust".to_string())
    );
    assert_eq!(served.get("/"), (200, "front:/:none".to_string()));

    // Hidden files and paths that climb out of the site are never served.
    assert_eq!(served.get("/.env").0, 404);
    assert_eq!(served.get("/%2e%2e/%2e%2e/etc/passwd").0, 404);
}

#[test]
fn a_form_post_reaches_the_script() {
    let served = Served::start(
        "form",
        &[(
            "save.php",
            "<?php http_response_code(201); echo $_SERVER['REQUEST_METHOD'], ' ', $_POST['title'];",
        )],
    );
    let response = served.request(
        "POST",
        "/save.php",
        "Content-Type: application/x-www-form-urlencoded\r\n",
        "title=Hello+Phera",
    );
    assert_eq!(response, (201, "POST Hello Phera".to_string()));
}

/// Many clients at once, on two worker threads: every request gets its own
/// answer and none sees another's variables.
#[test]
fn concurrent_requests_are_isolated() {
    let served = Served::start(
        "concurrent",
        &[(
            "echo.php",
            "<?php $id = $_GET['id']; $sum = 0; for ($i = 0; $i < 20000; $i++) { $sum += $i; } echo $id, ':', $sum;",
        )],
    );
    std::thread::scope(|scope| {
        let clients: Vec<_> = (0..40)
            .map(|id| {
                let served = &served;
                scope.spawn(move || (id, served.get(&format!("/echo.php?id={id}"))))
            })
            .collect();
        for client in clients {
            let (id, response) = client.join().expect("client thread");
            assert_eq!(response, (200, format!("{id}:199990000")));
        }
    });
}

/// Sixty requests that each wait 300 ms, sent at once to ONE worker thread.
/// One after the other they would take 18 seconds. Because `usleep` waits
/// without blocking the thread, they all finish in well under two.
#[test]
fn waiting_requests_do_not_block_each_other() {
    let served = Served::start_with_workers(
        "waiting",
        1,
        &[(
            "wait.php",
            "<?php usleep(300000); echo 'done ', $_GET['id'];",
        )],
    );
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        let clients: Vec<_> = (0..60)
            .map(|id| {
                let served = &served;
                scope.spawn(move || (id, served.get(&format!("/wait.php?id={id}"))))
            })
            .collect();
        for client in clients {
            let (id, response) = client.join().expect("client thread");
            assert_eq!(response, (200, format!("done {id}")));
        }
    });
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "60 waiting requests took {elapsed:?} on one thread"
    );
}

/// A request that computes for a long time shares its thread: a quick
/// request sent while it runs is answered long before it finishes.
#[test]
fn a_long_computation_lets_other_requests_through() {
    let served = Served::start_with_workers(
        "fairness",
        1,
        &[
            (
                "slow.php",
                "<?php $s = 0; for ($i = 0; $i < 30000000; $i++) { $s += $i % 7; } echo $s;",
            ),
            ("quick.php", "<?php echo 'quick';"),
        ],
    );
    std::thread::scope(|scope| {
        let slow = scope.spawn(|| {
            let started = std::time::Instant::now();
            (served.get("/slow.php"), started.elapsed())
        });
        // Give the slow request time to start, then ask for the quick one.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let started = std::time::Instant::now();
        assert_eq!(served.get("/quick.php"), (200, "quick".to_string()));
        let quick_took = started.elapsed();
        let (slow_response, slow_took) = slow.join().expect("slow client");
        assert_eq!(slow_response, (200, "89999995".to_string()));
        assert!(
            quick_took * 4 < slow_took,
            "the quick request took {quick_took:?}, the slow one {slow_took:?}"
        );
    });
}

/// Three findings of the security test of the server, each fixed and kept
/// fixed here.
#[test]
fn hostile_requests_do_not_get_source_wrong_framing_or_a_filled_post() {
    let served = Served::start(
        "hardening",
        &[
            ("secret.PHP", "<?php $password = 'do-not-leak'; echo 'ran';"),
            (
                "length.php",
                "<?php header('Content-Length: 5'); echo str_repeat('A', 100);",
            ),
            (
                "form.php",
                "<?php echo count($_POST), ' ', $_SERVER['REQUEST_METHOD'];",
            ),
        ],
    );

    // An upper-case extension runs the script. Serving it as a file would
    // print its source.
    assert_eq!(served.get("/secret.PHP"), (200, "ran".to_string()));

    // A script cannot announce a shorter body than it sends: the client gets
    // all 100 bytes, framed by the server's own length.
    let (status, body) = served.get("/length.php");
    assert_eq!((status, body.len()), (200, 100));

    // Only POST fills `$_POST`, as in PHP.
    let form = "Content-Type: application/x-www-form-urlencoded\r\n";
    assert_eq!(
        served.request("PUT", "/form.php", form, "a=1&b=2"),
        (200, "0 PUT".to_string())
    );
    assert_eq!(
        served.request("POST", "/form.php", form, "a=1&b=2"),
        (200, "2 POST".to_string())
    );
}
