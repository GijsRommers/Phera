//! Taking a `multipart/form-data` body apart: the format a browser uses
//! for a form that uploads files.
//!
//! ```text
//! Content-Type: multipart/form-data; boundary=XyZ
//!
//! --XyZ
//! Content-Disposition: form-data; name="title"
//!
//! Holiday
//! --XyZ
//! Content-Disposition: form-data; name="photo"; filename="beach.jpg"
//! Content-Type: image/jpeg
//!
//! (the bytes of the file)
//! --XyZ--
//! ```
//!
//! The rules are those of PHP's own reader, `main/rfc1867.c`, including
//! what it does with input that is cut off or malformed.

mod buffer;

use buffer::{Buffer, FILL_UNIT};

/// Names and headers of the part currently being read.
pub(super) struct PartHeaders {
    pub(super) disposition: Disposition,
    pub(super) content_type: Vec<u8>,
}

pub(super) enum Disposition {
    Absent,
    Form {
        name: Option<Vec<u8>>,
        filename: Option<Vec<u8>>,
    },
}

#[derive(Clone, Copy)]
enum MimeHeader {
    Disposition,
    ContentType,
}

/// The boundary of a multipart POST that PHP would parse on startup.
pub(super) fn boundary(content_type: &[u8]) -> Option<Vec<u8>> {
    super::media_type(content_type)
        .eq_ignore_ascii_case(b"multipart/form-data")
        .then(|| explicit_boundary(content_type).ok())
        .flatten()
        .filter(|boundary| !boundary.is_empty())
        .map(<[u8]>::to_vec)
}

pub(super) struct Multipart<'a> {
    buffer: Buffer<'a>,
    delimiter: Vec<u8>,
}

impl<'a> Multipart<'a> {
    pub(super) fn new(body: &'a [u8], boundary: &[u8]) -> Self {
        Self {
            buffer: Buffer::new(body, boundary.len()),
            delimiter: [b"\n--", boundary].concat(),
        }
    }

    /// Find the next delimiter line and read this part's MIME headers.
    pub(super) fn next_headers(&mut self) -> Option<PartHeaders> {
        loop {
            let line = c_string(self.buffer.line()?);
            if line == &self.delimiter[1..] {
                break;
            }
        }
        let mut disposition: Option<Vec<u8>> = None;
        let mut content_type: Option<Vec<u8>> = None;
        let mut current = None;
        while let Some(line) = self.buffer.line() {
            let line = c_string(line);
            if line.is_empty() {
                break;
            }
            let colon = (!line[0].is_ascii_whitespace())
                .then(|| line.iter().position(|&byte| byte == b':'))
                .flatten();
            let Some(colon) = colon else {
                let value = match current {
                    Some(MimeHeader::Disposition) => disposition.as_mut(),
                    Some(MimeHeader::ContentType) => content_type.as_mut(),
                    None => None,
                };
                if let Some(value) = value {
                    value.extend_from_slice(line);
                }
                continue;
            };
            let name = &line[..colon];
            let (header, target) = if name.eq_ignore_ascii_case(b"content-disposition") {
                (MimeHeader::Disposition, &mut disposition)
            } else if name.eq_ignore_ascii_case(b"content-type") {
                (MimeHeader::ContentType, &mut content_type)
            } else {
                current = None;
                continue;
            };
            current = if target.is_none() {
                *target = Some(line[colon + 1..].trim_ascii_start().to_vec());
                Some(header)
            } else {
                None
            };
        }
        let disposition = match disposition {
            Some(value) => {
                let mut name = None;
                let mut filename = None;
                for (key, value) in parameters_of(&value) {
                    match key.as_str() {
                        "name" => name = Some(value),
                        "filename" => filename = Some(value),
                        _ => {}
                    }
                }
                Disposition::Form { name, filename }
            }
            None => Disposition::Absent,
        };
        let content_type = content_type.unwrap_or_default();
        let content_type = content_type
            .split(|&byte| byte == b';')
            .next()
            .unwrap_or_default()
            .to_vec();
        Some(PartHeaders {
            disposition,
            content_type,
        })
    }

    pub(super) fn content(&mut self, limit: Option<usize>) -> (&'a [u8], bool) {
        self.buffer.content(&self.delimiter, limit)
    }

    pub(super) fn consumed(&self) -> usize {
        self.buffer.consumed()
    }
}

fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or_default()
}

/// The `key=value` parameters of a header value such as
/// `form-data; name="a"; filename="b \"c\".txt"`. Keys come back in lower
/// case. A value may stand in double or single quotes; inside them a
/// backslash keeps the quote or backslash after it.
fn parameters_of(value: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut parameters = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let Some(equals) = rest.iter().position(|&byte| byte == b'=' || byte == b';') else {
            break;
        };
        if rest[equals] == b';' {
            rest = &rest[equals + 1..];
            continue;
        }
        // As in PHP, a space before the `=` makes the key another key:
        // `name = "a"` names nothing.
        let key = String::from_utf8_lossy(rest[..equals].trim_ascii_start()).to_ascii_lowercase();
        let (parsed, after) = parameter_value(&rest[equals + 1..]);
        parameters.push((key, parsed));
        rest = after;
    }
    parameters
}

/// PHP searches for "boundary" anywhere in the header, then reads the value
/// through the next quote, semicolon or comma, depending on its opening byte.
pub(super) fn explicit_boundary(content_type: &[u8]) -> Result<&[u8], BoundaryError> {
    let start = find(content_type, b"boundary")
        .or_else(|| find(&content_type.to_ascii_lowercase(), b"boundary"))
        .ok_or(BoundaryError::Missing)?;
    let value = content_type[start..]
        .iter()
        .position(|&byte| byte == b'=')
        .map(|equals| &content_type[start + equals + 1..])
        .ok_or(BoundaryError::Missing)?;
    let boundary = if let Some(quoted) = value.strip_prefix(b"\"") {
        quoted
            .iter()
            .position(|&byte| byte == b'"')
            .map(|end| &quoted[..end])
            .ok_or(BoundaryError::Invalid)?
    } else {
        value
            .split(|&byte| matches!(byte, b';' | b','))
            .next()
            .unwrap_or_default()
    };
    if boundary.len() > FILL_UNIT - 4 {
        return Err(BoundaryError::TooLarge);
    }
    Ok(boundary)
}

pub(super) enum BoundaryError {
    Missing,
    Invalid,
    TooLarge,
}

/// One parameter value, quoted or bare, and the input after it.
fn parameter_value(input: &[u8]) -> (Vec<u8>, &[u8]) {
    let input = input.trim_ascii_start();
    let quoted = input
        .split_first()
        .filter(|(first, _)| matches!(**first, b'"' | b'\''));
    let Some((&quote, quoted)) = quoted else {
        let end = input
            .iter()
            .position(|&byte| byte == b';')
            .unwrap_or(input.len());
        let value = input[..end]
            .split(|byte| byte.is_ascii_whitespace())
            .next()
            .unwrap_or_default();
        return (value.to_vec(), &input[end..]);
    };
    let mut value = Vec::new();
    let mut index = 0;
    while let Some(&byte) = quoted.get(index) {
        match (byte, quoted.get(index + 1)) {
            (b'\\', Some(&next)) if next == b'\\' || next == quote => {
                value.push(next);
                index += 2;
            }
            _ if byte == quote => return (value, &quoted[index + 1..]),
            _ => {
                value.push(byte);
                index += 1;
            }
        }
    }
    (value, &[])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Part<'a> {
        name: Option<Vec<u8>>,
        filename: Option<Vec<u8>>,
        content_type: Vec<u8>,
        content: &'a [u8],
        complete: bool,
    }

    fn parts<'a>(body: &'a [u8], boundary: &[u8]) -> Vec<Part<'a>> {
        let mut form = Multipart::new(body, boundary);
        let mut parts = Vec::new();
        while let Some(headers) = form.next_headers() {
            let Disposition::Form { name, filename } = headers.disposition else {
                continue;
            };
            let (content, complete) = form.content(None);
            parts.push(Part {
                name,
                filename,
                content_type: headers.content_type,
                content,
                complete,
            });
        }
        parts
    }

    #[test]
    fn the_boundary_is_read_from_the_content_type() {
        let boundary_of = |header: &str| boundary(header.as_bytes());
        assert_eq!(
            boundary_of("multipart/form-data; boundary=XyZ"),
            Some(b"XyZ".to_vec())
        );
        assert_eq!(
            boundary_of("MULTIPART/FORM-DATA; charset=utf-8; BOUNDARY=\"X y\""),
            Some(b"X y".to_vec())
        );
        for other in [
            "multipart/form-data",
            "multipart/form-data; boundary=",
            "multipart/mixed; boundary=XyZ",
            "application/x-www-form-urlencoded",
            "",
        ] {
            assert_eq!(boundary_of(other), None, "{other}");
        }
    }

    #[test]
    fn a_field_and_a_file() {
        let body = b"preamble\r\n--XyZ\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\n\
            Holiday\r\n--XyZ\r\ncontent-disposition: form-data; name=photo; \
            filename=\"C:\\\\tmp\\\\a \\\"b\\\".jpg\"\r\nContent-Type: image/jpeg; x=1\r\n\r\n\
            line1\r\nnot --XyZ at the start of a line\r\n\x00\xff\r\n--XyZ--\r\nepilogue";
        let parts = parts(body, b"XyZ");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name.as_deref(), Some(&b"title"[..]));
        assert_eq!(
            (parts[0].filename.as_ref(), parts[0].content),
            (None, &b"Holiday"[..])
        );
        assert_eq!(parts[1].name.as_deref(), Some(&b"photo"[..]));
        assert_eq!(
            parts[1].filename.as_deref(),
            Some(&b"C:\\tmp\\a \"b\".jpg"[..])
        );
        assert_eq!(parts[1].content_type, b"image/jpeg");
        let spaced = super::parameters_of(b"form-data; name = \"a\"; filename='it''s'; x=bare ;y");
        assert_eq!(
            spaced,
            [
                ("name ".to_string(), b"a".to_vec()),
                ("filename".to_string(), b"it".to_vec()),
                ("x".to_string(), b"bare".to_vec()),
            ]
        );
        assert_eq!(
            parts[1].content,
            b"line1\r\nnot --XyZ at the start of a line\r\n\x00\xff"
        );
        assert!(parts.iter().all(|part| part.complete));
    }

    #[test]
    fn a_body_that_is_cut_off_keeps_what_arrived() {
        let body = b"--B\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n--B\r\n\
            Content-Disposition: form-data; name=\"f\"; filename=\"x.txt\"\r\n\r\npartial";
        let parts = parts(body, b"B");
        assert_eq!(parts.len(), 2);
        assert!(parts[0].complete);
        assert_eq!(
            (parts[1].content, parts[1].complete),
            (&b"partial"[..], false)
        );
    }

    #[test]
    fn what_is_not_a_form_gives_no_parts() {
        assert!(parts(b"", b"B").is_empty());
        assert!(parts(b"no boundary here", b"B").is_empty());
        assert!(parts(b"--B--\r\n", b"B").is_empty());
        assert!(parts(b"--B\r\nContent-Disposition: form-data", b"B").is_empty());
        // Line feeds alone, and a part with no name.
        let bare = parts(
            b"--B\nContent-Disposition: form-data; filename=a\n\nx\n--B--",
            b"B",
        );
        assert_eq!(bare.len(), 1);
        assert_eq!((bare[0].name.as_ref(), bare[0].content), (None, &b"x"[..]));
    }
}
