//! Compressing a response while it is sent: `phera serve --compress`.
//!
//! ```text
//! GET /products.json                    Content-Encoding: br
//! Accept-Encoding: gzip, br             84 kB of JSON travel as 9 kB
//! ```
//!
//! Off unless asked for: the proxy in front of Phera usually compresses,
//! and doing it twice only costs time. A file that has a compressed copy
//! next to it (`app.js.br`, see `precompressed`) never comes here.

use std::io::Write;

use phera_server::{Body, Response};

use super::precompressed::{self, Encoding};

/// Below this a response fits in one network packet either way.
const SMALLEST_WORTH_COMPRESSING: usize = 1024;

/// Above this a PHP response is compressed on another thread, so that the
/// worker thread stays free for other requests meanwhile.
const LARGEST_COMPRESSED_IN_PLACE: usize = 64 * 1024;

/// What this server compresses with while a request waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compressor {
    Brotli,
    Gzip,
}

impl Compressor {
    /// The one to use for a client that sent this `Accept-Encoding`:
    /// brotli when it takes it, otherwise gzip.
    pub(crate) fn for_client(accept_encoding: Option<&str>) -> Option<Compressor> {
        precompressed::accepted(accept_encoding)
            .into_iter()
            .find_map(|encoding| match encoding {
                Encoding::Brotli => Some(Compressor::Brotli),
                Encoding::Gzip => Some(Compressor::Gzip),
                Encoding::Zstd => None,
            })
    }

    pub(crate) fn encoding(self) -> Encoding {
        match self {
            Compressor::Brotli => Encoding::Brotli,
            Compressor::Gzip => Encoding::Gzip,
        }
    }

    /// Compresses `bytes`. The levels are the ones that cost about as much
    /// time as PHP spends producing a page: more would hold up the response
    /// for a few percent of size.
    pub(crate) fn compress(self, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len() / 4);
        // Writing into a `Vec` cannot fail, so the results are not inspected.
        match self {
            Compressor::Gzip => {
                let level = flate2::Compression::new(6);
                let mut encoder = flate2::write::GzEncoder::new(&mut output, level);
                let _ = encoder.write_all(bytes);
                let _ = encoder.finish();
            }
            Compressor::Brotli => {
                const QUALITY: u32 = 5;
                const WINDOW_BITS: u32 = 22;
                let mut encoder =
                    brotli::CompressorWriter::new(&mut output, 4096, QUALITY, WINDOW_BITS);
                let _ = encoder.write_all(bytes);
                let _ = encoder.flush();
            }
        }
        output
    }
}

/// Whether bytes of this type and length get smaller. Images, video, fonts
/// and archives are compressed already.
pub(crate) fn is_worthwhile(content_type: &str, length: u64) -> bool {
    if length < SMALLEST_WORTH_COMPRESSING as u64 {
        return false;
    }
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type.starts_with("text/")
        || media_type.ends_with("+json")
        || media_type.ends_with("+xml")
        || matches!(
            media_type.as_str(),
            "application/json"
                | "application/javascript"
                | "application/xml"
                | "application/wasm"
                | "application/x-ndjson"
                | "image/x-icon"
                | "font/ttf"
                | "font/otf"
        )
}

/// Compresses what a PHP script answered, when that is allowed and useful.
pub(crate) async fn php_response(
    mut response: Response,
    accept_encoding: Option<&str>,
) -> Response {
    let content_type = text_header(&response, "content-type");
    let length = match &response.body {
        Body::Bytes(bytes) => bytes.len(),
        Body::File(_) => return response,
    };
    if !has_whole_plain_body(&response) || !is_worthwhile(&content_type, length as u64) {
        return response;
    }
    // From here on the answer depends on the request's `Accept-Encoding`,
    // whether or not this client gets it compressed.
    add_vary(&mut response);
    let Some(compressor) = Compressor::for_client(accept_encoding) else {
        return response;
    };
    let Body::Bytes(original) = std::mem::take(&mut response.body) else {
        return response;
    };
    let (original, compressed) = if length <= LARGEST_COMPRESSED_IN_PLACE {
        let compressed = compressor.compress(&original);
        (original, compressed)
    } else {
        let on_another_thread = tokio::task::spawn_blocking(move || {
            let compressed = compressor.compress(&original);
            (original, compressed)
        });
        match on_another_thread.await {
            Ok(both) => both,
            Err(_) => return Response::text(500, "Internal Server Error\n"),
        }
    };
    if compressed.len() >= original.len() {
        return response.with_body(original);
    }
    weaken_etag(&mut response);
    response
        .with_header("Content-Encoding", compressor.encoding().token())
        .with_body(compressed)
}

/// A complete, uncompressed body: not a part (206), not "nothing" (204,
/// 304), and not something the script compressed itself.
fn has_whole_plain_body(response: &Response) -> bool {
    !matches!(response.status, 100..=199 | 204 | 206 | 304)
        && response.header("content-encoding").is_none()
        && response.header("content-range").is_none()
}

/// A script's `ETag: "abc"` promised these exact bytes. Compressed, they are
/// other bytes with the same meaning, which is what `W/"abc"` says.
fn weaken_etag(response: &mut Response) {
    for (name, value) in &mut response.headers {
        if name.eq_ignore_ascii_case("etag") && value.starts_with(b"\"") {
            value.splice(0..0, *b"W/");
        }
    }
}

fn add_vary(response: &mut Response) {
    let already = text_header(response, "vary")
        .split(',')
        .any(|name| name.trim().eq_ignore_ascii_case("accept-encoding") || name.trim() == "*");
    if !already {
        response
            .headers
            .push(("Vary".to_string(), b"Accept-Encoding".to_vec()));
    }
}

fn text_header(response: &Response, name: &str) -> String {
    response
        .header(name)
        .map(|value| String::from_utf8_lossy(value).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn page() -> Vec<u8> {
        "<li class=\"product\">A product with a description</li>\n"
            .repeat(400)
            .into_bytes()
    }

    #[test]
    fn both_encodings_shrink_text_and_decode_to_the_same_bytes() {
        let original = page();

        let gzip = Compressor::Gzip.compress(&original);
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(gzip.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, original);
        assert!(gzip.len() * 10 < original.len());

        let brotli = Compressor::Brotli.compress(&original);
        let mut decoded = Vec::new();
        brotli::Decompressor::new(brotli.as_slice(), 4096)
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, original);
        assert!(brotli.len() * 10 < original.len());
    }

    #[test]
    fn brotli_is_preferred_and_zstd_alone_is_not_enough() {
        let for_client = Compressor::for_client;
        assert_eq!(for_client(Some("gzip, br")), Some(Compressor::Brotli));
        assert_eq!(for_client(Some("gzip, zstd")), Some(Compressor::Gzip));
        assert_eq!(for_client(Some("zstd")), None);
        assert_eq!(for_client(Some("identity")), None);
        assert_eq!(for_client(None), None);
    }

    #[test]
    fn only_text_like_types_of_some_size_are_worth_it() {
        assert!(is_worthwhile("text/html; charset=UTF-8", 5000));
        assert!(is_worthwhile("Application/JSON", 5000));
        assert!(is_worthwhile("image/svg+xml", 5000));
        assert!(is_worthwhile("application/ld+json", 5000));
        assert!(!is_worthwhile("text/html", 1023));
        assert!(!is_worthwhile("image/png", 500_000));
        assert!(!is_worthwhile("video/mp4", 500_000));
        assert!(!is_worthwhile("font/woff2", 500_000));
        assert!(!is_worthwhile("application/zip", 500_000));
        assert!(!is_worthwhile("", 500_000));
    }

    fn html(body: Vec<u8>) -> Response {
        Response::new(200)
            .with_header("Content-Type", "text/html; charset=UTF-8")
            .with_body(body)
    }

    fn body_length(response: &Response) -> usize {
        match &response.body {
            Body::Bytes(bytes) => bytes.len(),
            Body::File(file) => file.length as usize,
        }
    }

    #[tokio::test]
    async fn a_page_is_compressed_for_a_client_that_accepts_it() {
        let response = php_response(html(page()), Some("gzip, br")).await;
        assert_eq!(response.header("content-encoding"), Some(&b"br"[..]));
        assert_eq!(response.header("vary"), Some(&b"Accept-Encoding"[..]));
        assert!(body_length(&response) < page().len() / 10);

        // Large enough to be compressed on another thread.
        let large = page().repeat(10);
        let response = php_response(html(large.clone()), Some("gzip")).await;
        assert_eq!(response.header("content-encoding"), Some(&b"gzip"[..]));
        assert!(body_length(&response) < large.len() / 10);
    }

    #[tokio::test]
    async fn what_must_stay_as_it_is_stays() {
        // The client takes neither: untouched, but marked as varying.
        let response = php_response(html(page()), Some("identity")).await;
        assert_eq!(response.header("content-encoding"), None);
        assert_eq!(response.header("vary"), Some(&b"Accept-Encoding"[..]));
        assert_eq!(body_length(&response), page().len());

        // The script compressed it itself, or sent only a part.
        let own = html(page()).with_header("Content-Encoding", "gzip");
        let response = php_response(own, Some("br")).await;
        assert_eq!(response.header_values("content-encoding").count(), 1);
        assert_eq!(body_length(&response), page().len());
        let mut part = html(page());
        part.status = 206;
        assert_eq!(
            body_length(&php_response(part, Some("br")).await),
            page().len()
        );

        // An image, and a body too small to matter.
        let image = Response::new(200)
            .with_header("Content-Type", "image/png")
            .with_body(page());
        let response = php_response(image, Some("br")).await;
        assert_eq!(response.header("content-encoding"), None);
        assert_eq!(response.header("vary"), None);
        let small = php_response(html(b"<p>hi</p>".to_vec()), Some("br")).await;
        assert_eq!(small.header("content-encoding"), None);
    }

    #[tokio::test]
    async fn a_script_tag_becomes_weak_and_vary_is_extended_once() {
        let tagged = html(page())
            .with_header("ETag", "\"v1\"")
            .with_header("Vary", "Cookie");
        let response = php_response(tagged, Some("br")).await;
        assert_eq!(response.header("etag"), Some(&b"W/\"v1\""[..]));
        let vary: Vec<_> = response.header_values("vary").collect();
        assert_eq!(vary, [&b"Cookie"[..], &b"Accept-Encoding"[..]]);

        let already = html(page()).with_header("Vary", "Cookie, accept-encoding");
        let response = php_response(already, Some("br")).await;
        assert_eq!(response.header_values("vary").count(), 1);
    }
}
