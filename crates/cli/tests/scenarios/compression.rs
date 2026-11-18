//! `phera serve --compress`: gzip and brotli for clients that accept them,
//! and nothing of the kind without the flag.

use std::io::Read;

use super::super::Served;
use super::static_files::{Answer, fetch};

/// A page of 2,000 table rows, 150 kB: large enough to be compressed off
/// the worker thread.
const PAGE: &str = "<?php header('ETag: \"v1\"'); \
    for ($i = 0; $i < 2000; $i++) { \
        echo '<tr class=\"row\"><td>', $i, '</td><td><a href=\"/product/', $i, \
             '\">Product number ', $i, '</a></td></tr>', \"\\n\"; \
    }";
const SMALL: &str = "<?php echo '<p>hello</p>';";
const IMAGE: &str = "<?php header('Content-Type: image/png'); echo str_repeat('x', 5000);";

fn stylesheet() -> String {
    (0..400)
        .map(|index| format!(".item-{index} {{ margin: {index}px; color: teal; }}\n"))
        .collect()
}

fn site(name: &str, flags: &[&str]) -> Served {
    let styles = stylesheet();
    Served::start_in(
        name,
        2,
        &[
            ("page.php", PAGE),
            ("small.php", SMALL),
            ("image.php", IMAGE),
            ("site.css", &styles),
            ("copied.css", &styles),
            ("copied.css.gz", "a gzip copy made at build time"),
        ],
        ".",
        flags,
    )
}

/// What the client sees after undoing the `Content-Encoding`.
fn decoded(answer: &Answer) -> Vec<u8> {
    let mut plain = Vec::new();
    match answer.header("content-encoding") {
        "gzip" => flate2::read::GzDecoder::new(answer.body.as_slice())
            .read_to_end(&mut plain)
            .map(drop),
        "br" => brotli::Decompressor::new(answer.body.as_slice(), 4096)
            .read_to_end(&mut plain)
            .map(drop),
        _ => {
            plain.clone_from(&answer.body);
            Ok(())
        }
    }
    .expect("the body decodes");
    plain
}

#[test]
fn a_php_page_is_compressed_for_clients_that_accept_it() {
    let served = site("compress-php", &["--compress"]);
    let plain = fetch(&served, "GET", "/page.php", &[]);
    assert_eq!(plain.header("content-encoding"), "");
    assert!(plain.body.len() > 100_000);

    for (accept, encoding) in [
        ("gzip, deflate, br", "br"),
        ("gzip", "gzip"),
        ("br;q=0, gzip", "gzip"),
        ("zstd", ""),
    ] {
        let header = format!("Accept-Encoding: {accept}");
        let answer = fetch(&served, "GET", "/page.php", &[&header]);
        assert_eq!(answer.header("content-encoding"), encoding, "{accept}");
        assert_eq!(answer.header("vary"), "Accept-Encoding", "{accept}");
        assert_eq!(
            answer.header("content-length"),
            answer.body.len().to_string()
        );
        assert!(decoded(&answer) == plain.body, "{accept}: other bytes");
        if !encoding.is_empty() {
            assert!(answer.body.len() < plain.body.len() / 5, "{accept}");
            // The script's tag named the plain bytes.
            assert_eq!(answer.header("etag"), "W/\"v1\"", "{accept}");
        }
    }
}

#[test]
fn small_answers_and_images_are_left_alone() {
    let served = site("compress-skip", &["--compress"]);
    for path in ["/small.php", "/image.php"] {
        let answer = fetch(&served, "GET", path, &["Accept-Encoding: gzip, br"]);
        assert_eq!(answer.header("content-encoding"), "", "{path}");
    }
}

#[test]
fn a_static_file_is_compressed_and_cached_under_its_own_tag() {
    let served = site("compress-static", &["--compress"]);
    let styles = stylesheet();
    let plain = fetch(&served, "GET", "/site.css", &[]);
    let packed = fetch(&served, "GET", "/site.css", &["Accept-Encoding: gzip"]);
    assert_eq!(packed.header("content-encoding"), "gzip");
    assert_eq!(packed.header("content-type"), "text/css; charset=UTF-8");
    assert!(packed.body.len() < styles.len() / 3);
    assert_eq!(decoded(&packed), styles.as_bytes());
    assert_ne!(packed.header("etag"), plain.header("etag"));

    // The compressed copy is recognised again without compressing.
    let tag = format!("If-None-Match: {}", packed.header("etag"));
    let again = fetch(
        &served,
        "GET",
        "/site.css",
        &["Accept-Encoding: gzip", &tag],
    );
    assert_eq!(again.status, 304);

    // HEAD describes what GET would send.
    let head = fetch(&served, "HEAD", "/site.css", &["Accept-Encoding: gzip"]);
    assert_eq!(head.header("content-encoding"), "gzip");
    assert_eq!(head.header("content-length"), packed.body.len().to_string());

    // A part is cut from the plain file.
    let part = fetch(
        &served,
        "GET",
        "/site.css",
        &["Accept-Encoding: gzip", "Range: bytes=0-9"],
    );
    assert_eq!((part.status, part.header("content-encoding")), (206, ""));
    assert_eq!(part.body, &styles.as_bytes()[..10]);

    // A copy made at build time is sent as it is, not compressed again.
    let copied = fetch(&served, "GET", "/copied.css", &["Accept-Encoding: gzip"]);
    assert_eq!(copied.body, b"a gzip copy made at build time");
}

/// A text file too large to hold in memory twice goes out as it is.
#[test]
fn a_very_large_file_is_not_compressed_while_the_client_waits() {
    let served = site("compress-large", &["--compress"]);
    let line = "a line of a very large log file\n";
    let contents = line.repeat(5 * 1024 * 1024 / line.len());
    std::fs::write(served.root.join("large.txt"), &contents).unwrap();
    let answer = fetch(&served, "GET", "/large.txt", &["Accept-Encoding: gzip"]);
    assert_eq!(answer.header("content-encoding"), "");
    assert_eq!(answer.body.len(), contents.len());
}

#[test]
fn nothing_is_compressed_without_the_flag() {
    let served = site("compress-off", &[]);
    for path in ["/page.php", "/site.css"] {
        let answer = fetch(&served, "GET", path, &["Accept-Encoding: gzip, br"]);
        assert_eq!(answer.header("content-encoding"), "", "{path}");
    }
    let page = fetch(&served, "GET", "/page.php", &["Accept-Encoding: gzip"]);
    assert_eq!((page.header("vary"), page.header("etag")), ("", "\"v1\""));
    // A copy made at build time is still used: that costs nothing.
    let copied = fetch(&served, "GET", "/copied.css", &["Accept-Encoding: gzip"]);
    assert_eq!(copied.header("content-encoding"), "gzip");
}
