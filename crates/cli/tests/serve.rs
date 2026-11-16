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
        let root = std::env::temp_dir().join(format!("phera-serve-{name}-{}", std::process::id()));
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("create directory");
            std::fs::write(file, contents).expect("write file");
        }

        let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "2"])
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
