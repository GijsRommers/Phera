//! Mapping a request path to a file of the site.

use std::io;
use std::path::{Component, Path, PathBuf};

/// The directory being served, and the script that answers what no file
/// matches.
pub(crate) struct Site {
    /// Canonical, so "is this file inside the site?" is a prefix test.
    document_root: PathBuf,
    excluded: Vec<PathBuf>,
    /// Frameworks route every URL through one script ("front controller").
    /// It is `index.php` in the document root when that exists, or the
    /// script named on the command line.
    front_controller: Option<PathBuf>,
    /// True when `phera serve` was pointed directly at a single `.php` file.
    single_script: bool,
    /// Optional AOT image so scripts stored inside the image resolve even when
    /// PHP source files have been stripped from disk.
    image: Option<std::sync::Arc<phera_runtime::AotImage>>,
}

/// What a request path leads to.
pub(crate) enum Resolved {
    /// Run this PHP file.
    Script(PathBuf),
    /// Send this file as it is.
    File(PathBuf),
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
                excluded: Vec::new(),
                front_controller: index.is_file().then_some(index),
                document_root: path,
                single_script: false,
                image: None,
            })
        } else {
            let document_root = path
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| io::Error::other("the script has no directory"))?;
            Ok(Site {
                document_root,
                excluded: Vec::new(),
                front_controller: Some(path),
                single_script: true,
                image: None,
            })
        }
    }

    /// Opens a virtual site backed by an AOT image when the target path may not
    /// exist on disk (e.g., strict stripped container deployments).
    pub(crate) fn open_with_image(
        path: &Path,
        image: std::sync::Arc<phera_runtime::AotImage>,
    ) -> io::Result<Site> {
        let mut site = match Self::open(path) {
            Ok(site) => site,
            Err(_) => {
                let root = PathBuf::from(&image.manifest().project_root);
                let mut front_controller = None;
                let mut single_script = false;
                let doc_root = if path == Path::new(".") {
                    // Prefer a conventional web front controller (`public/index.php` or `index.php`),
                    // never a CLI entry such as `bin/console` or `artisan`.
                    let web_entry = image
                        .find_relative("public/index.php")
                        .or_else(|| image.find_relative("index.php"))
                        .filter(|entry| entry.unit_web.is_some());
                    if let Some(entry) = web_entry {
                        let fc = PathBuf::from(&entry.canonical_path);
                        let dir = fc
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| root.clone());
                        front_controller = Some(fc);
                        dir
                    } else {
                        root.clone()
                    }
                } else {
                    let target = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        root.join(path)
                    };
                    if is_script(&target)
                        && let Some(entry) = image
                            .find_exact_canonical(&target)
                            .filter(|entry| entry.unit_web.is_some())
                    {
                        let fc = PathBuf::from(&entry.canonical_path);
                        let dir = fc
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| root.clone());
                        front_controller = Some(fc);
                        single_script = true;
                        dir
                    } else {
                        target
                    }
                };
                Site {
                    document_root: doc_root,
                    excluded: Vec::new(),
                    front_controller,
                    single_script,
                    image: None,
                }
            }
        };
        if site.front_controller.is_none() {
            let index = site.document_root.join("index.php");
            if let Some(entry) = image
                .find_exact_canonical(&index)
                .filter(|entry| entry.unit_web.is_some())
            {
                site.front_controller = Some(PathBuf::from(&entry.canonical_path));
            }
        }
        site.image = Some(image);
        Ok(site)
    }

    pub(crate) fn exclude_files(&mut self, paths: Vec<PathBuf>) {
        self.excluded = paths;
        if let Some(fc) = &self.front_controller {
            let excluded = self.excluded.contains(fc)
                || fc
                    .canonicalize()
                    .ok()
                    .is_some_and(|p| self.excluded.contains(&p));
            if excluded {
                self.front_controller = None;
            }
        }
    }

    /// The script that answers every path without a file of its own.
    pub(crate) fn front_controller(&self) -> Option<&Path> {
        self.front_controller.as_deref()
    }

    pub(crate) fn is_single_script(&self) -> bool {
        self.single_script
    }

    pub(crate) fn document_root(&self) -> &Path {
        &self.document_root
    }

    pub(crate) fn describe(&self) -> String {
        self.document_root.display().to_string()
    }

    /// Finds what a request path refers to. It asks the file system about
    /// the path (`stat`, `realpath`) but reads no file, so the worker thread
    /// calls it directly.
    pub(crate) fn resolve(&self, request_path: &str) -> Resolved {
        let fallback = || match &self.front_controller {
            Some(script) => Resolved::Script(script.clone()),
            None => Resolved::NotFound,
        };
        let Some(relative) = safe_relative_path(request_path) else {
            return Resolved::NotFound;
        };
        let mut candidate = self.document_root.join(relative);
        if let Some(image) = &self.image {
            if is_script(&candidate)
                && let Some(entry) = image
                    .find_exact_canonical(&candidate)
                    .filter(|entry| entry.unit_web.is_some())
            {
                let resolved = PathBuf::from(&entry.canonical_path);
                if self.is_allowed_image_script(&resolved) {
                    return Resolved::Script(resolved);
                }
                return Resolved::NotFound;
            }
            let dir_index = candidate.join("index.php");
            if let Some(entry) = image
                .find_exact_canonical(&dir_index)
                .filter(|entry| entry.unit_web.is_some())
            {
                let resolved = PathBuf::from(&entry.canonical_path);
                if self.is_allowed_image_script(&resolved) {
                    return Resolved::Script(resolved);
                }
                return Resolved::NotFound;
            }
        }
        match candidate.metadata() {
            Ok(metadata) if metadata.is_dir() => candidate.push("index.php"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return fallback(),
            _ => {}
        }
        // Resolving symbolic links first means a link that points outside
        // the site cannot be used to read files outside it.
        let Ok(file) = candidate.canonicalize() else {
            return fallback();
        };
        if self.excluded.contains(&file) || is_image_file(&file) {
            return Resolved::NotFound;
        }
        if !file.starts_with(&self.document_root) || !file.is_file() {
            return fallback();
        }
        if is_script(&file) {
            Resolved::Script(file)
        } else {
            Resolved::File(file)
        }
    }

    /// Verifies that an image-resolved script stays strictly inside
    /// `document_root`, is not excluded, and traverses no hidden or backslash
    /// path components.
    fn is_allowed_image_script(&self, path: &Path) -> bool {
        if self.excluded.iter().any(|excluded| excluded == path)
            || !path.starts_with(&self.document_root)
        {
            return false;
        }
        let Ok(rel) = path.strip_prefix(&self.document_root) else {
            return false;
        };
        rel.components().all(|component| match component {
            Component::Normal(name) => {
                let s = name.to_string_lossy();
                !s.starts_with('.') && !s.contains('\\')
            }
            _ => false,
        })
    }

    /// The real path of `candidate` when it is a file this site may send as
    /// it is: inside the document root, not excluded, not a script, and not a
    /// `.phb` AOT image.
    pub(crate) fn plain_file(&self, candidate: &Path) -> Option<PathBuf> {
        let file = candidate.canonicalize().ok()?;
        let allowed = file.starts_with(&self.document_root)
            && !self.excluded.contains(&file)
            && file.is_file()
            && !is_script(&file)
            && !is_image_file(&file);
        allowed.then_some(file)
    }
}

/// `.PHP` and `.Php` run as scripts too, as in PHP's own server. Serving
/// them as files would hand out their source, with whatever passwords it
/// contains.
fn is_script(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("php"))
}

/// `.phb` AOT images contain compiled bytecode and every string literal of the
/// application; never serve them as downloadable static files.
fn is_image_file(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("phb"))
}

/// Turns `/css/site.css` into `css/site.css`. Returns `None` for a path that
/// must not be served: one that climbs out with `..`, one that names a hidden
/// file such as `.env` or `.git/config`, or one that is not valid text.
fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request_path)?;
    let mut relative = PathBuf::new();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(name) => {
                let s = name.to_string_lossy();
                if s.starts_with('.') || s.contains('\\') {
                    return None;
                }
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
