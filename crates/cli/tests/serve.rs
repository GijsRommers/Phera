//! `phera serve` from the outside: start the real binary, talk HTTP to it
//! over a socket, and check what comes back.

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
}

impl Served {
    fn start(name: &str, files: &[(&str, &str)]) -> Served {
        Served::start_with_workers(name, 2, files)
    }

    fn start_with_workers(name: &str, workers: usize, files: &[(&str, &str)]) -> Served {
        let root = std::env::temp_dir().join(format!("phera-serve-{name}-{}", std::process::id()));
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("create directory");
            std::fs::write(file, contents).expect("write file");
        }

        let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
            .args(["serve", "--listen", "127.0.0.1:0", "--workers"])
            .arg(workers.to_string())
            .arg(&root)
            .stdout(Stdio::piped())
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
        let mut stream = TcpStream::connect(&self.address).expect("connect");
        write!(
            stream,
            "{method} {target} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\
             Content-Length: {}\r\n{extra_headers}\r\n{body}",
            self.address,
            body.len()
        )
        .expect("send request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");

        let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or_else(|| panic!("no status in {head:?}"));
        (status, body.to_string())
    }

    fn get(&self, target: &str) -> (u16, String) {
        self.request("GET", target, "", "")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
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

#[test]
fn environment_snapshot_and_putenv_are_isolated_on_one_worker() {
    let served = Served::start_with_workers(
        "env",
        1,
        &[
            (".env", "PHERA_TEST_COLOR=green"),
            (
                "put.php",
                "<?php putenv('PHERA_TEST_COLOR=red'); echo getenv('PHERA_TEST_COLOR');",
            ),
            (
                "get.php",
                "<?php echo getenv('PHERA_TEST_COLOR'), $_ENV['PHERA_TEST_COLOR'], \
         $_SERVER['PHERA_TEST_COLOR'];",
            ),
        ],
    );
    assert_eq!(served.get("/put.php"), (200, "red".into()));
    std::fs::write(served.root.join(".env"), "PHERA_TEST_COLOR=blue").unwrap();
    assert_eq!(served.get("/get.php"), (200, "greengreengreen".into()));
}
