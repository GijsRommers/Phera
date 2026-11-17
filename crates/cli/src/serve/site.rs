//! Mapping a request path to a file of the site.

use std::io;
use std::path::{Component, Path, PathBuf};

/// The directory being served, and the script that answers what no file
/// matches.
pub(crate) struct Site {
    /// Canonical, so "is this file inside the site?" is a prefix test.
    document_root: PathBuf,
    /// Frameworks route every URL through one script ("front controller").
    /// It is `index.php` in the document root when that exists, or the
    /// script named on the command line.
    front_controller: Option<PathBuf>,
}

/// What a request path leads to.
pub(crate) enum Resolved {
    /// Run this PHP file.
    Script(PathBuf),
    /// Send this file as it is.
    File {
        content_type: &'static str,
        body: Vec<u8>,
    },
    NotFound,
}

impl Site {
    /// `path` is a directory to serve, or a single script that answers every
    /// request (its directory is then the document root).
    pub(crate) fn open(path: &Path) -> io::Result<Site> {
        let path = path.canonicalize()?;
        if path.is_dir() {
            let index = path.join("index.php");
            Ok(Site {
                front_controller: index.is_file().then_some(index),
                document_root: path,
            })
        } else {
            let document_root = path
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| io::Error::other("the script has no directory"))?;
            Ok(Site {
                document_root,
                front_controller: Some(path),
            })
        }
    }

    pub(crate) fn document_root(&self) -> &Path {
        &self.document_root
    }

    pub(crate) fn describe(&self) -> String {
        self.document_root.display().to_string()
    }

    /// Finds what a request path refers to. Reads the file system, so call
    /// it from a blocking thread.
    pub(crate) fn resolve(&self, request_path: &str) -> Resolved {
        let fallback = || match &self.front_controller {
            Some(script) => Resolved::Script(script.clone()),
            None => Resolved::NotFound,
        };
        let Some(relative) = safe_relative_path(request_path) else {
            return Resolved::NotFound;
        };
        let mut candidate = self.document_root.join(relative);
        if candidate.is_dir() {
            candidate.push("index.php");
        }
        // Resolving symbolic links first means a link that points outside
        // the site cannot be used to read files outside it.
        let Ok(file) = candidate.canonicalize() else {
            return fallback();
        };
        if !file.starts_with(&self.document_root) || !file.is_file() {
            return fallback();
        }
        // `.PHP` and `.Php` run as scripts too, as in PHP's own server. Serving
        // them as files would hand out their source, with whatever passwords
        // it contains.
        let is_script = file
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("php"));
        if is_script {
            return Resolved::Script(file);
        }
        match std::fs::read(&file) {
            Ok(body) => Resolved::File {
                content_type: content_type(&file),
                body,
            },
            Err(_) => Resolved::NotFound,
        }
    }
}

/// Turns `/css/site.css` into `css/site.css`. Returns `None` for a path that
/// must not be served: one that climbs out with `..`, one that names a hidden
/// file such as `.env` or `.git/config`, or one that is not valid text.
fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request_path)?;
    let mut relative = PathBuf::new();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(name) if !name.to_string_lossy().starts_with('.') => {
                relative.push(name);
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(relative)
}

/// Decodes `%20` and friends in a URL path. Unlike in a query string, `+`
/// stays a plus. `None` if the result is not UTF-8 or contains a NUL byte.
fn percent_decode(path: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(path.len());
    let mut rest = path.as_bytes();
    while let [first, tail @ ..] = rest {
        rest = tail;
        if let (b'%', [high, low, after @ ..]) = (first, tail)
            && let (Some(high), Some(low)) = (hex_digit(*high), hex_digit(*low))
        {
            bytes.push(high * 16 + low);
            rest = after;
        } else {
            bytes.push(*first);
        }
    }
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

fn content_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("html" | "htm") => "text/html; charset=UTF-8",
        Some("css") => "text/css; charset=UTF-8",
        Some("js" | "mjs") => "text/javascript; charset=UTF-8",
        Some("json" | "map") => "application/json",
        Some("txt") => "text/plain; charset=UTF-8",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_that_could_leave_the_site_or_reveal_secrets_are_refused() {
        assert_eq!(
            safe_relative_path("/css/site.css"),
            Some(PathBuf::from("css/site.css"))
        );
        assert_eq!(
            safe_relative_path("/a%20b.txt"),
            Some(PathBuf::from("a b.txt"))
        );
        assert_eq!(safe_relative_path("/"), Some(PathBuf::new()));
        assert_eq!(safe_relative_path("/../etc/passwd"), None);
        assert_eq!(safe_relative_path("/a/%2e%2e/%2e%2e/etc/passwd"), None);
        assert_eq!(safe_relative_path("/.env"), None);
        assert_eq!(safe_relative_path("/.git/config"), None);
        assert_eq!(safe_relative_path("/a%00.php"), None);
        assert_eq!(safe_relative_path("/%ff"), None);
    }
}
