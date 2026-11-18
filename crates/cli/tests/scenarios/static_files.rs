//! Static files through `phera serve`: caching, ranges, `HEAD`,
//! precompressed copies, and files too large to hold in memory.

use std::io::{Read, Write};
use std::net::TcpStream;

use super::super::Served;

/// A response with its headers, which the shared helper does not keep.
struct Answer {
    status: u16,
    /// Names in lower case.
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> &str {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_str())
    }
}

/// Sends one request with the given extra header lines.
fn fetch(served: &Served, method: &str, path: &str, headers: &[&str]) -> Answer {
    let mut stream = TcpStream::connect(&served.address).expect("connect");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        served.address
    );
    for header in headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).expect("send request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");

    let end_of_head = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a response head");
    let head = String::from_utf8_lossy(&response[..end_of_head]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a status code");
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
        .collect();
    Answer {
        status,
        headers,
        body: response[end_of_head + 4..].to_vec(),
    }
}

const STYLES: &str = "body { color: teal }\n";

fn site(name: &str) -> Served {
    Served::start(
        name,
        &[
            ("css/site.css", STYLES),
            ("index.php", "<?php echo 'front';"),
        ],
    )
}

#[test]
fn a_file_comes_with_what_a_cache_needs() {
    let served = site("static-get");
    let answer = fetch(&served, "GET", "/css/site.css", &[]);
    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, STYLES.as_bytes());
    assert_eq!(answer.header("content-type"), "text/css; charset=UTF-8");
    assert_eq!(answer.header("content-length"), STYLES.len().to_string());
    assert_eq!(answer.header("accept-ranges"), "bytes");
    assert_eq!(answer.header("vary"), "Accept-Encoding");
    assert!(answer.header("etag").starts_with('"'));
    assert!(answer.header("last-modified").ends_with(" GMT"));

    // HEAD: the same headers, the length included, and no body.
    let head = fetch(&served, "HEAD", "/css/site.css", &[]);
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length"), STYLES.len().to_string());
    assert_eq!(head.header("etag"), answer.header("etag"));
    assert!(head.body.is_empty());

    // A file takes no data.
    let post = fetch(&served, "POST", "/css/site.css", &["Content-Length: 0"]);
    assert_eq!((post.status, post.header("allow")), (405, "GET, HEAD"));
}

#[test]
fn an_unchanged_file_is_not_sent_again() {
    let served = site("static-304");
    let first = fetch(&served, "GET", "/css/site.css", &[]);
    let tag = format!("If-None-Match: {}", first.header("etag"));
    let date = format!("If-Modified-Since: {}", first.header("last-modified"));

    for condition in [tag.as_str(), date.as_str(), "If-None-Match: *"] {
        let again = fetch(&served, "GET", "/css/site.css", &[condition]);
        assert_eq!(again.status, 304, "{condition}");
        assert!(again.body.is_empty(), "{condition}");
        assert_eq!(again.header("etag"), first.header("etag"), "{condition}");
    }
    // The tag decides alone: another tag means "send it", whatever the date.
    let other = fetch(
        &served,
        "GET",
        "/css/site.css",
        &["If-None-Match: \"other\"", &date],
    );
    assert_eq!(other.status, 200);

    // After a change the old tag no longer matches.
    std::fs::write(served.root.join("css/site.css"), "body { color: navy; }\n").unwrap();
    let changed = fetch(&served, "GET", "/css/site.css", &[&tag]);
    assert_eq!(changed.status, 200);
    assert_ne!(changed.header("etag"), first.header("etag"));
    assert_eq!(changed.body, b"body { color: navy; }\n");
}

#[test]
fn a_range_sends_part_of_the_file() {
    let served = site("static-range");
    let total = STYLES.len();
    // The header, the status, `Content-Range`, and the bytes expected.
    let cases: [(&str, u16, String, &str); 8] = [
        ("bytes=7-11", 206, format!("bytes 7-11/{total}"), "color"),
        ("bytes=14-", 206, format!("bytes 14-20/{total}"), "teal }\n"),
        ("bytes=-3", 206, format!("bytes 18-20/{total}"), " }\n"),
        (
            "bytes=14-9999",
            206,
            format!("bytes 14-20/{total}"),
            "teal }\n",
        ),
        ("bytes=0-0", 206, format!("bytes 0-0/{total}"), "b"),
        // Not understood, so ignored: the whole file.
        ("bytes=0-1,5-6", 200, String::new(), STYLES),
        ("lines=1-2", 200, String::new(), STYLES),
        ("bytes=9-2", 200, String::new(), STYLES),
    ];
    for (range, status, content_range, body) in cases {
        let answer = fetch(
            &served,
            "GET",
            "/css/site.css",
            &[&format!("Range: {range}")],
        );
        assert_eq!(answer.status, status, "{range}");
        assert_eq!(answer.header("content-range"), content_range, "{range}");
        assert_eq!(answer.body, body.as_bytes(), "{range}");
        assert_eq!(answer.header("content-length"), body.len().to_string());
    }

    let beyond = fetch(&served, "GET", "/css/site.css", &["Range: bytes=21-"]);
    assert_eq!(beyond.status, 416);
    assert_eq!(beyond.header("content-range"), format!("bytes */{total}"));

    // HEAD ignores a range.
    let head = fetch(&served, "HEAD", "/css/site.css", &["Range: bytes=7-11"]);
    assert_eq!((head.status, head.header("content-range")), (200, ""));
}

/// A download is resumed only if the file is still the version the client
/// has the first part of.
#[test]
fn if_range_guards_a_resumed_download() {
    let served = site("static-if-range");
    let first = fetch(&served, "GET", "/css/site.css", &[]);
    let same = format!("If-Range: {}", first.header("etag"));
    let resumed = fetch(&served, "GET", "/css/site.css", &["Range: bytes=5-", &same]);
    assert_eq!(resumed.status, 206);

    let stale = fetch(
        &served,
        "GET",
        "/css/site.css",
        &["Range: bytes=5-", "If-Range: \"an-older-version\""],
    );
    assert_eq!(
        (stale.status, stale.body.as_slice()),
        (200, STYLES.as_bytes())
    );
}

#[test]
fn a_precompressed_copy_is_sent_to_a_client_that_accepts_it() {
    let served = Served::start(
        "static-precompressed",
        &[
            ("app.js", "console.log('plain');"),
            ("app.js.br", "brotli bytes"),
            ("app.js.gz", "gzip bytes"),
            ("only.js", "console.log('only');"),
        ],
    );
    let cases = [
        ("Accept-Encoding: gzip, deflate, br", "br", "brotli bytes"),
        ("Accept-Encoding: gzip", "gzip", "gzip bytes"),
        ("Accept-Encoding: br;q=0, gzip", "gzip", "gzip bytes"),
        ("Accept-Encoding: identity", "", "console.log('plain');"),
        ("Accept-Encoding: zstd", "", "console.log('plain');"),
        ("X-Nothing: 1", "", "console.log('plain');"),
    ];
    let mut tags = Vec::new();
    for (header, encoding, body) in cases {
        let answer = fetch(&served, "GET", "/app.js", &[header]);
        assert_eq!(answer.status, 200, "{header}");
        assert_eq!(answer.header("content-encoding"), encoding, "{header}");
        assert_eq!(answer.body, body.as_bytes(), "{header}");
        // The type is the script's, whatever the bytes are packed in.
        assert_eq!(
            answer.header("content-type"),
            "text/javascript; charset=UTF-8"
        );
        assert_eq!(answer.header("vary"), "Accept-Encoding");
        tags.push(answer.header("etag").to_string());
    }
    // Each packing has its own tag, so a cache never mixes them up.
    assert_ne!(tags[0], tags[1]);
    assert_ne!(tags[1], tags[3]);

    // No copy next to the file: it goes out as it is.
    let plain = fetch(&served, "GET", "/only.js", &["Accept-Encoding: gzip, br"]);
    assert_eq!(plain.header("content-encoding"), "");
    assert_eq!(plain.body, b"console.log('only');");
}

/// A "compressed copy" that is really a link to a file outside the site is
/// not followed.
#[cfg(unix)]
#[test]
fn a_copy_that_points_outside_the_site_is_not_used() {
    let served = Served::start("static-link", &[("app.js", "console.log('plain');")]);
    let outside = served.root.with_extension("outside");
    std::fs::write(&outside, "not part of the site").unwrap();
    std::os::unix::fs::symlink(&outside, served.root.join("app.js.gz")).unwrap();

    let answer = fetch(&served, "GET", "/app.js", &["Accept-Encoding: gzip"]);
    std::fs::remove_file(&outside).unwrap();
    assert_eq!(answer.header("content-encoding"), "");
    assert_eq!(answer.body, b"console.log('plain');");
}

/// Eight megabytes arrive complete and in order, whole and in part: the
/// file is sent in pieces, never as one block.
#[test]
fn a_large_file_arrives_intact() {
    let served = site("static-large");
    let contents: Vec<u8> = (0..8 * 1024 * 1024u32)
        .map(|index| (index.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    std::fs::write(served.root.join("large.bin"), &contents).unwrap();

    let whole = fetch(&served, "GET", "/large.bin", &[]);
    assert_eq!(whole.header("content-length"), contents.len().to_string());
    assert!(whole.body == contents, "the whole file differs");

    let part = fetch(
        &served,
        "GET",
        "/large.bin",
        &["Range: bytes=3000000-5999999"],
    );
    assert_eq!(part.status, 206);
    assert!(
        part.body == contents[3_000_000..6_000_000],
        "the part differs"
    );

    let empty = served.root.join("empty.txt");
    std::fs::write(empty, "").unwrap();
    let nothing = fetch(&served, "GET", "/empty.txt", &[]);
    assert_eq!(
        (nothing.status, nothing.header("content-length")),
        (200, "0")
    );
    assert!(nothing.body.is_empty());
}
