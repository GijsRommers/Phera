//! Starting `phera serve` on a directory of PHP files and sending it many
//! requests at once, to show that waits for I/O overlap on few workers.
//!
//! ```no_run
//! use phera_extension_api::testing::serve::Server;
//!
//! let root = std::env::temp_dir().join("example-site");
//! std::fs::create_dir_all(&root).unwrap();
//! std::fs::write(root.join("index.php"), "<?php echo 'done';").unwrap();
//! let server = Server::start("target/release/phera".as_ref(), &root, 1, &[]);
//! let responses = server.get_all((0..40).map(|index| format!("/?n={index}")));
//! assert!(responses.iter().all(|response| response.starts_with("HTTP/1.1 200")));
//! ```

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// A running `phera serve`, stopped when dropped.
pub struct Server {
    process: Child,
    /// `127.0.0.1:<port>`, as the server's banner printed it.
    pub address: String,
}

impl Server {
    /// Serves `root` on a free port with `workers` worker threads, with the
    /// extra `environment` variables set.
    pub fn start(phera: &Path, root: &Path, workers: usize, environment: &[(&str, &str)]) -> Self {
        let mut process = Command::new(phera)
            .args(["--php", super::oracle::test_level().name(), "serve"])
            .args(["--listen", "127.0.0.1:0", "--workers"])
            .arg(workers.to_string())
            .arg(root)
            .envs(environment.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start phera serve");
        let mut banner = String::new();
        BufReader::new(process.stdout.take().expect("piped stdout"))
            .read_line(&mut banner)
            .expect("read the listen banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in the banner {banner:?}"))
            .to_owned();
        Self { process, address }
    }

    /// PHP's built-in server (`php -n -S`) on `root` with `workers`
    /// processes, to compare `phera serve` with.
    pub fn start_php(php: &Path, root: &Path, workers: usize) -> Self {
        Self::start_php_with_router(php, root, workers, None)
    }

    /// Runs PHP's built-in server with a PHP script routing every request.
    pub fn start_php_router(php: &Path, root: &Path, router: &Path) -> Self {
        Self::start_php_with_router(php, root, 1, Some(router))
    }

    fn start_php_with_router(
        php: &Path,
        root: &Path,
        workers: usize,
        router: Option<&Path>,
    ) -> Self {
        // Ask the system for a free port, then hand it to PHP.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("find a free port")
            .port();
        let address = format!("127.0.0.1:{port}");
        let mut command = Command::new(php);
        command.args(["-n", "-S", &address, "-t"]).arg(root);
        if workers > 1 {
            command.env("PHP_CLI_SERVER_WORKERS", workers.to_string());
        } else {
            command.env_remove("PHP_CLI_SERVER_WORKERS");
        }
        if let Some(router) = router {
            command.arg(router);
        }
        let process = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start php -S");
        let server = Self { process, address };
        for _ in 0..200 {
            if TcpStream::connect(&server.address).is_ok() {
                return server;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("php -S did not start on {}", server.address);
    }

    /// The raw response to one `GET`.
    pub fn get_one(&self, path: &str) -> String {
        self.get(path)
    }

    /// The raw response to one POST with a byte body.
    pub fn post_one(&self, path: &str, body: &[u8]) -> String {
        self.retry_request(path, || self.try_request("POST", path, &[], body))
    }

    /// One request with optional additional headers and a body, as raw HTTP.
    pub fn request(
        &self,
        method: &str,
        target: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> String {
        self.try_request(method, target, headers, body)
            .expect("send the request and read its response")
    }

    /// Sends a `GET` for every path at the same time, one connection each,
    /// and returns the raw responses in the order of `paths`.
    pub fn get_all(&self, paths: impl IntoIterator<Item = String>) -> Vec<String> {
        let paths: Vec<String> = paths.into_iter().collect();
        std::thread::scope(|scope| {
            let requests: Vec<_> = paths
                .iter()
                .map(|path| scope.spawn(|| self.get(path)))
                .collect();
            requests
                .into_iter()
                .map(|request| request.join().expect("request thread"))
                .collect()
        })
    }

    fn get(&self, path: &str) -> String {
        self.retry_request(path, || self.try_request("GET", path, &[], &[]))
    }

    fn retry_request(&self, path: &str, send: impl Fn() -> std::io::Result<String>) -> String {
        // macOS queues at most 128 connections a server has not accepted
        // yet (`kern.ipc.somaxconn`), and resets the rest of a burst; PHP's
        // built-in server queues fewer still. A request reset before any of
        // its response arrived is sent again, after a growing pause. On
        // Linux such a dropped connection can also end without an error and
        // without a byte, so an empty response counts as a reset: no HTTP
        // server answers a request with nothing.
        for attempt in 1..=RESETS_RETRIED {
            match send() {
                Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
                Ok(response) if response.is_empty() => {}
                response => return response.expect("send the request and read its response"),
            }
            // Let the server drain its queue before trying again.
            std::thread::sleep(RESET_PAUSE * attempt);
        }
        panic!("{path}: the connection was reset or closed unanswered {RESETS_RETRIED} times");
    }

    fn try_request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> std::io::Result<String> {
        let mut stream = self.connect()?;
        // Write the complete frame together so both test servers see the same request.
        let mut request = Vec::new();
        write!(
            request,
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n"
        )?;
        for (name, value) in headers {
            write!(request, "{name}: {value}\r\n")?;
        }
        if method == "POST" || !body.is_empty() {
            write!(request, "Content-Length: {}\r\n", body.len())?;
        }
        request.extend_from_slice(b"\r\n");
        request.extend_from_slice(body);
        stream.write_all(&request)?;
        read_response(stream, path)
    }

    fn connect(&self) -> std::io::Result<TcpStream> {
        let stream = TcpStream::connect(&self.address)?;
        // A server that hangs fails the test instead of stalling it.
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        Ok(stream)
    }
}

fn read_response(mut stream: TcpStream, path: &str) -> std::io::Result<String> {
    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Err(error) if !response.is_empty() => {
            panic!("{path}: the response broke off: {error}\n{response}")
        }
        result => result.map(|_| response),
    }
}

/// How often one request is sent after its connection was reset, and the
/// pause before the first retry, which grows with each.
const RESETS_RETRIED: u32 = 5;
const RESET_PAUSE: Duration = Duration::from_millis(50);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
