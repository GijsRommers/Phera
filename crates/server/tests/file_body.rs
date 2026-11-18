//! Responses whose body is part of a file on disk.

mod common;

use std::future::Future;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

/// 300,000 bytes: several chunks, and not a multiple of the chunk size.
fn contents() -> Vec<u8> {
    (0..300_000u32).map(|index| (index % 251) as u8).collect()
}

/// Answers `/from/<start>/<length>` with that part of the file.
struct SendFile {
    path: PathBuf,
}

impl Handler for SendFile {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            let mut numbers = request
                .path
                .split('/')
                .filter_map(|part| part.parse::<u64>().ok());
            let (start, length) = (numbers.next().unwrap(), numbers.next().unwrap());
            let mut file = std::fs::File::open(&self.path).unwrap();
            file.seek(SeekFrom::Start(start)).unwrap();
            Response::new(200).with_file(file, length)
        })
    }
}

struct SendFileFactory {
    path: PathBuf,
}

impl HandlerFactory for SendFileFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(SendFile {
            path: self.path.clone(),
        })
    }
}

fn file_on_disk(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("phera-server-{name}-{}", std::process::id()));
    std::fs::File::create(&path)
        .and_then(|mut file| file.write_all(&contents()))
        .unwrap();
    path
}

/// Three answers on one connection. If one sent a byte too many or too few,
/// the next would not parse.
#[test]
fn parts_of_a_file_arrive_with_their_exact_length() {
    let path = file_on_disk("parts");
    let (server, addr) = common::start(SendFileFactory { path: path.clone() }, 1, 1024);
    let mut client = Client::connect(addr);
    let expected = contents();

    for (start, length) in [(0, 300_000), (65_536, 100_000), (299_999, 1), (10, 0)] {
        let response = client.request(&format!("GET /from/{start}/{length} HTTP/1.1"), &[], b"");
        assert_eq!(response.status, 200);
        assert_eq!(
            response.header("content-length"),
            Some(length.to_string().as_str())
        );
        let (start, length) = (start as usize, length as usize);
        assert!(
            response.body == expected[start..start + length],
            "bytes {start}+{length} differ"
        );
    }
    server.shutdown();
    std::fs::remove_file(path).unwrap();
}

/// The file is shorter than the handler promised. The client must not get
/// a response that looks complete.
#[test]
fn a_file_that_ends_early_breaks_the_connection() {
    let path = file_on_disk("short");
    let (server, addr) = common::start(SendFileFactory { path: path.clone() }, 1, 1024);
    let mut client = Client::connect(addr);
    client.send("GET /from/0/400000 HTTP/1.1", &[], b"");
    let head = client.read_response_head();
    assert_eq!(head.header("content-length"), Some("400000"));
    let received = client.read_until_closed();
    assert!(received < 400_000, "{received} bytes arrived");
    server.shutdown();
    std::fs::remove_file(path).unwrap();
}
