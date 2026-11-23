//! Answering a request for a file of the site.
//!
//! ```text
//! GET /app.js                              200, the file, with ETag and Last-Modified
//! GET /app.js + If-None-Match: <its tag>   304, no body
//! GET /film.mp4 + Range: bytes=1000-1999   206, those thousand bytes
//! HEAD /app.js                             200, the headers only
//! POST /app.js                             405: a file takes no data
//! ```
//!
//! With `--compress`, a text-like file that has no precompressed copy is
//! compressed while the request waits, if it is small enough to hold in
//! memory. A request for a part of it gets the plain file: cutting a range
//! out of compressed bytes would mean compressing the whole file first.
//!
//! The file is opened once and everything is read from that open file: its
//! length, its time, and later its bytes. A deploy that replaces the file
//! meanwhile cannot make the headers describe one version and the body
//! another.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use phera_server::{Request, Response};

use super::byte_range::{self, RangeRequest};
use super::compress::{self, Compressor};
use super::conditional::Validators;
use super::precompressed::{self, Encoding};
use super::site::Site;

/// The parts of a request that decide how a file is answered.
pub(crate) struct Conditions {
    method: Method,
    if_none_match: Option<String>,
    if_modified_since: Option<String>,
    range: Option<String>,
    if_range: Option<String>,
    accept_encoding: Option<String>,
    /// `--compress` was given.
    compress: bool,
}

/// A file larger than this is not compressed while a request waits: it
/// would have to be held in memory twice. Its build step should write a
/// compressed copy next to it instead.
const LARGEST_FILE_COMPRESSED: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Head,
    Other,
}

impl Conditions {
    pub(crate) fn of(request: &Request, compress: bool) -> Conditions {
        let header = |name: &str| {
            request
                .header(name)
                .map(|value| String::from_utf8_lossy(value).into_owned())
        };
        Conditions {
            method: match request.method.as_str() {
                "GET" => Method::Get,
                "HEAD" => Method::Head,
                _ => Method::Other,
            },
            if_none_match: header("if-none-match"),
            if_modified_since: header("if-modified-since"),
            range: header("range"),
            if_range: header("if-range"),
            accept_encoding: header("accept-encoding"),
            compress,
        }
    }

    /// What to compress a plain file of this type and length with, if at
    /// all, for this request.
    fn compressor_for(&self, content_type: &str, length: u64) -> Option<Compressor> {
        let wanted = self.compress
            && self.range.is_none()
            && length <= LARGEST_FILE_COMPRESSED
            && compress::is_worthwhile(content_type, length);
        if wanted {
            Compressor::for_client(self.accept_encoding.as_deref())
        } else {
            None
        }
    }
}

/// The bytes that will be sent for a file: the file itself or a
/// precompressed copy of it.
struct Representation {
    file: File,
    length: u64,
    validators: Validators,
    encoding: Option<Encoding>,
    /// Set when the file is to be compressed now; `encoding` then names
    /// what the client will receive, not what is on disk.
    compress_with: Option<Compressor>,
}

/// Builds the response for `path`, a file of `site`. Reads the file system,
/// so call it from a blocking thread.
pub(crate) fn answer(site: &Site, path: &Path, conditions: &Conditions) -> Response {
    if conditions.method == Method::Other {
        return Response::text(405, "Method Not Allowed\n").with_header("Allow", "GET, HEAD");
    }
    let content_type = super::content_type::of(path);
    let Some(mut chosen) = choose(site, path, content_type, conditions) else {
        return Response::text(404, "Not Found\n");
    };

    let validators = &chosen.validators;
    let unchanged = validators.unchanged_for(
        conditions.if_none_match.as_deref(),
        conditions.if_modified_since.as_deref(),
    );
    if unchanged {
        return with_validators(Response::new(304), validators);
    }

    let mut response = Response::new(200).with_header("Content-Type", content_type);
    if let Some(encoding) = chosen.encoding {
        response = response.with_header("Content-Encoding", encoding.token());
    }
    response = with_validators(response, validators);

    if let Some(compressor) = chosen.compress_with {
        let mut contents = Vec::with_capacity(chosen.length as usize);
        if chosen.file.read_to_end(&mut contents).is_err() {
            return Response::text(500, "Internal Server Error\n");
        }
        return response.with_body(compressor.compress(&contents));
    }

    response = response.with_header("Accept-Ranges", "bytes");
    match requested_range(conditions, &chosen) {
        RangeRequest::Whole => response.with_file(chosen.file, chosen.length),
        RangeRequest::Part { start, length } => {
            if chosen.file.seek(SeekFrom::Start(start)).is_err() {
                return Response::text(500, "Internal Server Error\n");
            }
            let last = start + length - 1;
            let content_range = format!("bytes {start}-{last}/{}", chosen.length);
            response.status = 206;
            response
                .with_header("Content-Range", content_range)
                .with_file(chosen.file, length)
        }
        RangeRequest::Unsatisfiable => Response::text(416, "Range Not Satisfiable\n")
            .with_header("Content-Range", format!("bytes */{}", chosen.length)),
    }
}

/// `ETag`, `Last-Modified` and `Vary`, which a 200 and a 304 share. `Vary`
/// tells caches that the answer depends on `Accept-Encoding`.
fn with_validators(response: Response, validators: &Validators) -> Response {
    response
        .with_header("ETag", validators.etag.as_str())
        .with_header("Last-Modified", validators.last_modified())
        .with_header("Vary", "Accept-Encoding")
}

/// The part of the file to send. A range is honoured for `GET` only, and
/// with `If-Range` only when the client's copy is of this very version:
/// otherwise the pieces it has and the piece it asks for would not fit.
fn requested_range(conditions: &Conditions, chosen: &Representation) -> RangeRequest {
    let same_version = conditions
        .if_range
        .as_deref()
        .is_none_or(|if_range| chosen.validators.is_named_by(if_range));
    if conditions.method != Method::Get || !same_version {
        return RangeRequest::Whole;
    }
    byte_range::parse(conditions.range.as_deref(), chosen.length)
}

/// Opens the best precompressed copy the client accepts, or the file,
/// which may then be marked for compressing now.
fn choose(
    site: &Site,
    path: &Path,
    content_type: &str,
    conditions: &Conditions,
) -> Option<Representation> {
    let precompressed = precompressed::accepted(conditions.accept_encoding.as_deref())
        .into_iter()
        .find_map(|encoding| {
            let copy = site.plain_file(&with_suffix(path, encoding.file_suffix()))?;
            open(&copy, Some(encoding))
        });
    if precompressed.is_some() {
        return precompressed;
    }
    let mut plain = open(path, None)?;
    if let Some(compressor) = conditions.compressor_for(content_type, plain.length) {
        let encoding = compressor.encoding();
        plain.validators = plain
            .validators
            .of_variant(&format!("-{}", encoding.token()));
        plain.encoding = Some(encoding);
        plain.compress_with = Some(compressor);
    }
    Some(plain)
}

fn open(path: &Path, encoding: Option<Encoding>) -> Option<Representation> {
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    // The tag of a compressed copy differs from the tag of the file.
    let variant = encoding.map_or(String::new(), |encoding| format!("-{}", encoding.token()));
    Some(Representation {
        validators: Validators::of(metadata.modified().ok()?, metadata.len(), &variant),
        length: metadata.len(),
        file,
        encoding,
        compress_with: None,
    })
}

/// `app.js` and `.br` give `app.js.br`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}
