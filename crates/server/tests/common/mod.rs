//! Shared helpers for the integration tests: a tiny blocking HTTP/1.1
//! client over a plain `std::net::TcpStream`, no async runtime needed in
//! the tests themselves.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use phera_server::{HandlerFactory, Server, ServerConfig, ServerHandle};

/// Start a server on a random local port with the given factory.
pub fn start(
    factory: impl HandlerFactory,
    workers: usize,
    max_body_bytes: usize,
) -> (ServerHandle, SocketAddr) {
    let config = ServerConfig {
        addr: "127.0.0.1:0".parse().unwrap(),
        workers,
        max_body_bytes,
        header_read_timeout: Duration::from_secs(10),
        shutdown_timeout: Duration::from_secs(5),
    };
    let server = Server::bind(config, factory).unwrap();
    let addr = server.local_addr();
    (server.spawn(), addr)
}

/// A parsed HTTP/1.1 response.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn headers_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

/// A blocking client that can send several requests on one connection,
/// reconnecting whenever the server closed the previous one.
pub struct Client {
    addr: SocketAddr,
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    /// The last response said `connection: close`: reconnect before the next.
    closed: bool,
}

impl Client {
    pub fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        Client {
            addr,
            writer: stream,
            reader,
            closed: false,
        }
    }

    fn reconnect_if_closed(&mut self) {
        if self.closed {
            let stream = TcpStream::connect(self.addr).unwrap();
            self.reader = BufReader::new(stream.try_clone().unwrap());
            self.writer = stream;
            self.closed = false;
        }
    }

    /// Send a request: a request line, extra header lines and a body.
    pub fn send(&mut self, request_line: &str, headers: &[&str], body: &[u8]) {
        self.reconnect_if_closed();
        let mut request = format!("{request_line}\r\nHost: test\r\n");
        for header in headers {
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        self.writer.write_all(request.as_bytes()).unwrap();
        self.writer.write_all(body).unwrap();
        self.writer.flush().unwrap();
    }

    /// Send a request and read one response.
    pub fn request(&mut self, request_line: &str, headers: &[&str], body: &[u8]) -> Response {
        self.send(request_line, headers, body);
        self.read_response()
    }

    /// Read one response delimited by Content-Length.
    pub fn read_response(&mut self) -> Response {
        let (mut response, length) = self.read_head();
        let mut body = vec![0; length];
        self.reader.read_exact(&mut body).unwrap();
        response.body = body;
        self.closed = response.header("connection") == Some("close");
        response
    }

    /// Read one response head without reading the body (for HEAD requests).
    pub fn read_response_head(&mut self) -> Response {
        let (response, _length) = self.read_head();
        response
    }

    /// Read until the server closes the connection or breaks it, and
    /// return how many bytes arrived.
    pub fn read_until_closed(&mut self) -> usize {
        let mut received = 0;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            match self.reader.read(&mut buffer) {
                Ok(0) | Err(_) => return received,
                Ok(count) => received += count,
            }
        }
    }

    /// Assert that no body bytes arrive within a short window: the server
    /// keeps the connection open (keep-alive) but sends nothing.
    pub fn assert_no_body(&mut self) {
        self.writer
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let mut byte = [0u8; 1];
        let result = self.reader.get_mut().read(&mut byte);
        match result {
            Ok(0) => {} // closed cleanly, no body
            Ok(n) => panic!("unexpected {n} body byte(s)"),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => panic!("unexpected read error: {error}"),
        }
    }

    fn read_head(&mut self) -> (Response, usize) {
        let mut status_line = String::new();
        self.reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.starts_with("HTTP/1.1 ") || status_line.starts_with("HTTP/1.0 "),
            "not an HTTP/1.x response: {status_line:?}"
        );
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .expect("status line has a code")
            .parse()
            .expect("status code is a number");
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
            }
        }
        let length = headers
            .iter()
            .find(|(name, _)| name == "content-length")
            .and_then(|(_, value)| value.parse().ok())
            .unwrap_or(0);
        (
            Response {
                status,
                headers,
                body: Vec::new(),
            },
            length,
        )
    }
}
