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

use std::io::{BufRead, BufReader, Read, Write};
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
            .args(["serve", "--listen", "127.0.0.1:0", "--workers"])
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
        let mut stream = TcpStream::connect(&self.address).expect("connect to phera serve");
        // A server that hangs fails the test instead of stalling it.
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("set a read timeout");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .expect("send the request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the response");
        response
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
