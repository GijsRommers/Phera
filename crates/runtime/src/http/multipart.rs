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

/// One part of the body: a form field, or a file when `filename` is set.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Part<'a> {
    /// The form control's name; `None` if the part did not give one.
    pub(crate) name: Option<Vec<u8>>,
    /// The file's name as the client sent it, possibly with a path.
    pub(crate) filename: Option<Vec<u8>>,
    /// The media type the client claimed, without parameters; empty if none.
    pub(crate) content_type: Vec<u8>,
    pub(crate) content: &'a [u8],
    /// False when the body ended before this part's closing boundary: the
    /// upload was cut off.
    pub(crate) complete: bool,
}

/// The boundary named in a `Content-Type: multipart/form-data; boundary=...`
/// header, or `None` if it is another type or names no boundary.
pub(crate) fn boundary(content_type: &[u8]) -> Option<Vec<u8>> {
    let semicolon = content_type.iter().position(|&byte| byte == b';')?;
    let (media_type, parameters) = content_type.split_at(semicolon);
    if !media_type
        .trim_ascii()
        .eq_ignore_ascii_case(b"multipart/form-data")
    {
        return None;
    }
    parameters_of(parameters)
        .into_iter()
        .find(|(name, _)| name == "boundary")
        .map(|(_, value)| value)
        .filter(|boundary| !boundary.is_empty())
}

/// Splits `body` at `--boundary` lines.
pub(crate) fn parts<'a>(body: &'a [u8], boundary: &[u8]) -> Vec<Part<'a>> {
    let delimiter = [b"--", boundary].concat();
    let mut parts = Vec::new();
    // Whatever precedes the first boundary is not part of the form.
    let Some(first) = find(body, &delimiter) else {
        return parts;
    };
    let mut rest = &body[first + delimiter.len()..];
    loop {
        // `--boundary--` closes the form.
        if rest.starts_with(b"--") {
            break;
        }
        let Some(after_line) = skip_line(rest) else {
            break;
        };
        let Some((headers, after_headers)) = split_headers(after_line) else {
            break;
        };
        let (content, next) = match content_before(after_headers, &delimiter) {
            Some((content, next)) => (content, Some(next)),
            None => (after_headers, None),
        };
        let disposition = parameters_of(header(headers, b"content-disposition"));
        let parameter = |wanted: &str| {
            disposition
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value.clone())
        };
        let claimed_type = header(headers, b"content-type");
        parts.push(Part {
            name: parameter("name"),
            filename: parameter("filename"),
            content_type: claimed_type
                .split(|&byte| byte == b';')
                .next()
                .unwrap_or_default()
                .trim_ascii()
                .to_vec(),
            content,
            complete: next.is_some(),
        });
        match next {
            Some(next) => rest = next,
            None => break,
        }
    }
    parts
}

/// The rest after the end of the current line.
fn skip_line(input: &[u8]) -> Option<&[u8]> {
    let newline = input.iter().position(|&byte| byte == b'\n')?;
    Some(&input[newline + 1..])
}

/// The header lines of a part, and what follows the empty line after them.
fn split_headers(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut end = 0;
    loop {
        let line_end = end + input[end..].iter().position(|&byte| byte == b'\n')?;
        let line = input[end..line_end].trim_ascii_end();
        if line.is_empty() {
            return Some((&input[..end], &input[line_end + 1..]));
        }
        end = line_end + 1;
    }
}

/// The content up to the next boundary line, and what follows the boundary.
/// The line break before the boundary belongs to the boundary.
fn content_before<'a>(input: &'a [u8], delimiter: &[u8]) -> Option<(&'a [u8], &'a [u8])> {
    let mut from = 0;
    loop {
        let at = from + find(&input[from..], delimiter)?;
        // A boundary only counts at the start of a line.
        if at == 0 || input[at - 1] == b'\n' {
            let content = input[..at].strip_suffix(b"\n").unwrap_or(&input[..at]);
            let content = content.strip_suffix(b"\r").unwrap_or(content);
            return Some((content, &input[at + delimiter.len()..]));
        }
        from = at + 1;
    }
}

/// The value of the header `name` (given in lower case) among `headers`.
fn header<'a>(headers: &'a [u8], name: &[u8]) -> &'a [u8] {
    headers
        .split(|&byte| byte == b'\n')
        .find_map(|line| {
            let colon = line.iter().position(|&byte| byte == b':')?;
            line[..colon]
                .trim_ascii()
                .eq_ignore_ascii_case(name)
                .then(|| line[colon + 1..].trim_ascii())
        })
        .unwrap_or_default()
}

/// The `key=value` parameters of a header value such as
/// `form-data; name="a"; filename="b \"c\".txt"`. Keys come back in lower
/// case. A value may stand in double or single quotes; inside them a
/// backslash keeps the quote or backslash after it.
fn parameters_of(value: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut parameters = Vec::new();
    let mut rest = value;
    while let Some(semicolon) = rest.iter().position(|&byte| byte == b';') {
        rest = &rest[semicolon + 1..];
        let Some(equals) = rest.iter().position(|&byte| byte == b'=' || byte == b';') else {
            break;
        };
        if rest[equals] == b';' {
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
        return (input[..end].trim_ascii_end().to_vec(), &input[end..]);
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
