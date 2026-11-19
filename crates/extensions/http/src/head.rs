//! The status line and headers of the response being built.
//!
//! `header('Content-Type: application/json')` replaces the default MIME type.

use phera_extension_api::RequestState;

/// What `header()` and `http_response_code()` have decided so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    pub status: u16,
    /// In the order they were set. Names keep the spelling the script used;
    /// they are compared without regard to case.
    pub headers: Vec<(String, Vec<u8>)>,
    lines: Vec<Vec<u8>>,
}

impl Default for ResponseHead {
    fn default() -> Self {
        ResponseHead {
            status: 200,
            headers: Vec::new(),
            lines: Vec::new(),
        }
    }
}

impl ResponseHead {
    /// The web SAPI adds its default content type immediately before the header callback.
    pub fn prepare(state: &mut RequestState) {
        let head = state.get_or_default::<Self>();
        if head.header("Content-Type").is_none() {
            head.apply(b"Content-type: text/html; charset=UTF-8", false);
        }
    }

    /// PHP replaces the default status for a hidden fatal error.
    pub fn set_error_status(state: &mut RequestState) {
        let head = state.get_or_default::<Self>();
        if head.status == 200 {
            head.status = 500;
        }
    }

    /// Applies one `header()` line.
    ///
    /// Two forms are special in PHP. `HTTP/1.1 404 Not Found` sets the status
    /// instead of adding a header. `Location: ...` also turns the status into
    /// a redirect, unless the script already chose a 3xx status or 201.
    pub(crate) fn apply(&mut self, line: &[u8], replace: bool) {
        if let Some(status) = status_line(line) {
            self.status = status;
            return;
        }
        let Some(colon) = line.iter().position(|&byte| byte == b':') else {
            return;
        };
        let (name, rest) = line.split_at(colon);
        let name = String::from_utf8_lossy(name).trim().to_string();
        let value = rest.get(1..).unwrap_or_default().trim_ascii().to_vec();
        if name.is_empty() {
            return;
        }

        let already_redirecting = self.status == 201 || (300..400).contains(&self.status);
        if name.eq_ignore_ascii_case("location") && !already_redirecting {
            self.status = 302;
        }
        if replace {
            self.remove(&name);
        }
        self.headers.push((name, value));
        self.lines.push(line.to_vec());
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.headers
            .retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
        self.lines.retain(|line| {
            !line
                .split(|&byte| byte == b':')
                .next()
                .is_some_and(|existing| existing.trim_ascii().eq_ignore_ascii_case(name.as_bytes()))
        });
    }

    pub(crate) fn append_cookie(&mut self, value: &[u8]) {
        self.headers.push(("Set-Cookie".into(), value.to_vec()));
        self.lines.push([b"Set-Cookie: ", value].concat());
    }

    pub(crate) fn clear(&mut self) {
        self.headers.clear();
        self.lines.clear();
    }

    pub(crate) fn lines(&self) -> impl Iterator<Item = &[u8]> {
        self.lines.iter().map(Vec::as_slice)
    }

    /// The value of a header, if the script set it.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}

/// The status in a line such as `HTTP/1.1 404 Not Found`.
fn status_line(line: &[u8]) -> Option<u16> {
    let rest = line
        .get(..5)
        .filter(|prefix| prefix.eq_ignore_ascii_case(b"HTTP/"))
        .and_then(|_| line.get(5..))?;
    let code = rest.split(|&byte| byte == b' ').nth(1)?;
    let status: u16 = std::str::from_utf8(code).ok()?.parse().ok()?;
    (100..=599).contains(&status).then_some(status)
}
